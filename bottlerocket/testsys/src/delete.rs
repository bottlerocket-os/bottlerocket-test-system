use crate::error::{self, Result};
use http::StatusCode;
use kube::{core::object::HasStatus, Client, ResourceExt};
use model::{
    clients::{AllowNotFound, CrdClient, HttpStatusCode, ResourceClient, TestClient},
    TaskState,
};
use serde::Deserialize;
use serde_plain::derive_fromstr_from_deserialize;
use snafu::ResultExt;
use std::{collections::HashSet, time::Duration};
use structopt::StructOpt;
use topological_sort::TopologicalSort;

/// Delete an object from a testsys cluster.
#[derive(Debug, StructOpt)]
pub(crate) struct Delete {
    /// Delete all tests and resources from a testsys cluster.
    #[structopt(
        long,
        conflicts_with_all(&["object-type", "object-name", "include-resources", "force"])
    )]
    all: bool,

    /// Delete the resource object after a failed deletion attempt.
    /// Warning: the resources created must be manually removed.
    #[structopt(
        long,
        conflicts_with = "include-resources",
        requires_if("object-type", "resource")
    )]
    force: bool,

    /// The type of object that is being delete must be either `test` or `resource`.
    #[structopt(required_unless = "all")]
    object_type: Option<ObjectType>,

    /// The name of the test/resource that should be deleted.
    #[structopt(required_unless = "all")]
    object_name: Option<String>,

    /// Include this flag if all resources this object depends on should be deleted as well.
    #[structopt(long)]
    include_resources: bool,
}

#[derive(Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum ObjectType {
    Test,
    Resource,
}

derive_fromstr_from_deserialize!(ObjectType);

impl Delete {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        match (
            self.all,
            self.force,
            self.object_type,
            self.object_name.as_ref(),
        ) {
            (true, _, _, _) => delete_all(k8s_client).await,
            (false, false, Some(ObjectType::Test), Some(object_name)) => {
                delete_test(k8s_client, object_name, self.include_resources).await
            }
            (false, false, Some(ObjectType::Resource), Some(object_name)) => {
                delete_resource(k8s_client, object_name, self.include_resources).await
            }
            (false, true, Some(ObjectType::Resource), Some(object_name)) => {
                force_delete_resource(k8s_client, object_name).await
            }
            (_, _, _, _) => Err(error::Error::InvalidArguments {
                why: "Either `all` must be set, or both of `object-type` and `object-name`"
                    .to_string(),
            }),
        }
    }
}

async fn delete_all(k8s_client: Client) -> Result<()> {
    let mut failures = 0;
    let test_client = TestClient::new_from_k8s_client(k8s_client.clone());
    let resource_client = ResourceClient::new_from_k8s_client(k8s_client.clone());

    // Start by deleting all tests and waiting for their completion.
    println!("Deleting all tests...");
    test_client.delete_all().await.context(error::DeleteSnafu {
        what: "all tests".to_string(),
    })?;
    while !test_client
        .get_all()
        .await
        .context(error::GetSnafu { what: "all tests" })?
        .is_empty()
    {
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    println!("Test deletion complete.");
    // Build a topological sort with all resources in the cluster
    let mut deletion_order = all_resources_deletion_order(&k8s_client).await?;
    let mut awaiting_deletion = Vec::<String>::new();
    println!("Deleting all resources following dependencies...");
    while !deletion_order.is_empty() || !awaiting_deletion.is_empty() {
        // Check for all resources that have been deleted for completion.
        let mut still_awaiting = Vec::new();
        for resource_name in &awaiting_deletion {
            if let Some(resource) = resource_client
                .get(resource_name)
                .await
                .allow_not_found(|_| ())
                .context(error::GetSnafu {
                    what: resource_name,
                })?
            {
                // If the resource errored during deletion alert the user that a problem occured
                if resource
                    .status()
                    .map(|status| status.destruction.task_state == TaskState::Error)
                    .unwrap_or_default()
                {
                    failures += 1;
                    eprintln!("Resource '{}' failed to delete.", resource_name);
                    println!("Use `testsys delete resource {} --force` to manually clean up the resource.", resource_name);
                } else {
                    still_awaiting.push(resource_name.to_string());
                }
            } else {
                println!("Resource '{}' has been deleted", resource_name);
            }
        }
        awaiting_deletion = still_awaiting;

        // If all resources awaiting deletion have been deleted we can get a new
        // set of resources to delete.
        if awaiting_deletion.is_empty() {
            awaiting_deletion = deletion_order.pop_all();
            for resource in &awaiting_deletion {
                println!("Deleting resource '{}' ...", resource);
                resource_client
                    .delete(resource)
                    .await
                    .allow_not_found(|_| println!("The resource '{}' was not found", resource))
                    .context(error::DeleteSnafu {
                        what: resource.to_string(),
                    })?;
            }
        }
    }
    if failures == 0 {
        println!("All resources have been deleted.");
        Ok(())
    } else {
        eprintln!("Some resources failed to be deleted.");
        Err(error::Error::DeleteCommand { count: failures })
    }
}

/// Delete a testsys resource. If `include_resources`, all resources that this resource
/// depended on will also be deleted.
async fn delete_resource(
    k8s_client: Client,
    resource_name: &str,
    include_resources: bool,
) -> Result<()> {
    let delete_order = if include_resources {
        resource_dependency_delete_order(&k8s_client, vec![resource_name.to_string()]).await?
    } else {
        let mut topo_sort = TopologicalSort::new();
        topo_sort.insert(resource_name);
        topo_sort
    };
    delete_resources_in_order(&k8s_client, delete_order).await
}

/// Delete a testsys test. If `include_resources`, all resources that this test
/// depended on will also be deleted.
async fn delete_test(k8s_client: Client, test_name: &str, include_resources: bool) -> Result<()> {
    let test_client = TestClient::new_from_k8s_client(k8s_client.clone());
    let delete_order = if include_resources {
        Some(resource_deletion_order_for_test(k8s_client.clone(), test_name).await?)
    } else {
        None
    };

    if test_client
        .delete(test_name)
        .await
        .allow_not_found(|_| println!("The test '{}' was not found", test_name))
        .context(error::DeleteSnafu {
            what: test_name.to_string(),
        })?
        .is_some()
    {
        println!("Deleting test '{}'", test_name);
        test_client.wait_for_deletion(test_name).await;
        println!("Test '{}' has been deleted", test_name);
        if let Some(delete_order) = delete_order {
            return delete_resources_in_order(&k8s_client, delete_order).await;
        }
    }
    Ok(())
}

/// Delete all resources in a `TopologicalSort` in order. The function will wait
/// for each resource to finish before starting to delete the next resource.
async fn delete_resources_in_order(
    k8s_client: &Client,
    mut topo_sort: TopologicalSort<String>,
) -> Result<()> {
    let mut failures = 0;
    let resource_client = ResourceClient::new_from_k8s_client(k8s_client.clone());
    while !topo_sort.is_empty() {
        if let Some(independent_resource) = topo_sort.pop() {
            if resource_client
                .delete(&independent_resource)
                .await
                .allow_not_found(|_| {
                    println!("The resource '{}' was not found", independent_resource)
                })
                .context(error::DeleteSnafu {
                    what: independent_resource.to_string(),
                })?
                .is_some()
            {
                println!("Deleting resource '{}'", independent_resource);
                if resource_client
                    .wait_for_deletion(&independent_resource)
                    .await
                    .is_err()
                {
                    failures += 1;
                    eprintln!("Resource '{}' failed to delete.", independent_resource);
                    println!("Use `testsys delete resource {} --force` to manually clean up the resource.", independent_resource);
                } else {
                    println!("Resource '{}' has been deleted", independent_resource);
                }
            }
        }
    }
    if failures == 0 {
        Ok(())
    } else {
        eprintln!("Some resources failed to be deleted.");
        Err(error::Error::DeleteCommand { count: failures })
    }
}

/// Creates a `TopologicalSort` containing all resources that `test_name` depends on
/// which provides an order for deletion.
async fn resource_deletion_order_for_test(
    k8s_client: Client,
    test_name: &str,
) -> Result<TopologicalSort<String>> {
    let test_client = TestClient::new_from_k8s_client(k8s_client.clone());
    let get_test_result = test_client.get(test_name).await;

    if get_test_result.is_status_code(StatusCode::NOT_FOUND) {
        println!("Test '{}' does not exist, nothing to do.", test_name);
        return Ok(TopologicalSort::new());
    }

    let test = get_test_result.context(error::GetSnafu {
        what: test_name.to_string(),
    })?;

    resource_dependency_delete_order(&k8s_client, test.spec.resources).await
}

/// Creates a `TopologicalSort` containing all resources including `initial_resources` that
/// `initial_resources` depends on.
async fn resource_dependency_delete_order(
    k8s_client: &Client,
    initial_resources: Vec<String>,
) -> Result<TopologicalSort<String>> {
    let resource_client = ResourceClient::new_from_k8s_client(k8s_client.clone());
    let mut visited_resources = HashSet::<String>::from_iter(initial_resources.clone());
    let mut to_be_visited = initial_resources.clone();

    let mut topo_sort = TopologicalSort::new();

    while let Some(resource_name) = to_be_visited.pop() {
        let get_resource_result = resource_client.get(&resource_name).await;
        if get_resource_result.is_status_code(StatusCode::NOT_FOUND) {
            println!("Resource '{}' does not exist. Skipping.", resource_name);
            continue;
        }

        let resource = get_resource_result.context(error::GetSnafu {
            what: resource_name.to_string(),
        })?;

        if let Some(depended_resources) = resource.spec.depends_on {
            for depended_resource in depended_resources {
                topo_sort.add_dependency(resource_name.clone(), depended_resource.clone());

                visited_resources.insert(depended_resource.clone());
                to_be_visited.push(depended_resource);
            }
        } else {
            topo_sort.insert(resource_name);
        }
    }

    Ok(topo_sort)
}

/// Creates a `TopologicalSort` containing all resources in a testsys cluster.
async fn all_resources_deletion_order(k8s_client: &Client) -> Result<TopologicalSort<String>> {
    let mut topo_sort = TopologicalSort::new();
    let resource_client = ResourceClient::new_from_k8s_client(k8s_client.clone());
    let resources = resource_client.get_all().await.context(error::GetSnafu {
        what: "all resources",
    })?;
    for resource in resources {
        if let Some(depended_resources) = &resource.spec.depends_on {
            for depended_resource in depended_resources {
                topo_sort.add_dependency(resource.name(), depended_resource.clone());
            }
        } else {
            topo_sort.insert(resource.name());
        }
    }
    Ok(topo_sort)
}

/// Delete a resource from a testsys cluster by force.
/// The finalizers will be removed from the resource and the resource will be deleted.
/// The job for resource deletion will also be deleted.
/// This should only be used if a resource has already failed to delete.
async fn force_delete_resource(k8s_client: Client, resource_name: &str) -> Result<()> {
    let resource_client = ResourceClient::new_from_k8s_client(k8s_client.clone());
    resource_client
        .force_delete(resource_name)
        .await
        .context(error::DeleteSnafu {
            what: resource_name,
        })?;
    Ok(())
}
