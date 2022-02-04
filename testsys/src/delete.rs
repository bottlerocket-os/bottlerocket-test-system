use crate::error::{self, Result};
use kube::Client;
use model::clients::{CrdClient, ResourceClient, TestClient};
use serde::Deserialize;
use serde_plain::derive_fromstr_from_deserialize;
use snafu::ResultExt;
use std::collections::HashSet;
use structopt::StructOpt;
use topological_sort::TopologicalSort;

/// Delete an object from a testsys cluster.
#[derive(Debug, StructOpt)]
pub(crate) struct Delete {
    /// The type of object that is being delete must be either `test` or `resource`.
    #[structopt()]
    object_type: ObjectType,

    /// The name of the test/resource that should be deleted.
    #[structopt()]
    object_name: String,

    /// Include this flag if all resources this object depends on should be deleted as well.
    #[structopt(long)]
    include_resources: bool,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
enum ObjectType {
    Test,
    Resource,
}

derive_fromstr_from_deserialize!(ObjectType);

impl Delete {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        match self.object_type {
            ObjectType::Test => {
                delete_test(k8s_client, &self.object_name, self.include_resources).await
            }
            ObjectType::Resource => {
                delete_resource(k8s_client, &self.object_name, self.include_resources).await
            }
        }
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
    test_client
        .delete(test_name)
        .await
        .context(error::DeleteSnafu {
            what: test_name.to_string(),
        })?;

    println!("Deleting test '{}'", test_name);
    test_client.wait_for_deletion(test_name).await;
    println!("Test '{}' has been deleted", test_name);
    if let Some(delete_order) = delete_order {
        delete_resources_in_order(&k8s_client, delete_order).await
    } else {
        Ok(())
    }
}

/// Delete all resources in a `TopologicalSort` in order. The function will wait
/// for each resource to finish before starting to delete the next resource.
async fn delete_resources_in_order(
    k8s_client: &Client,
    mut topo_sort: TopologicalSort<String>,
) -> Result<()> {
    let resource_client = ResourceClient::new_from_k8s_client(k8s_client.clone());
    while !topo_sort.is_empty() {
        if let Some(independent_resource) = topo_sort.pop() {
            resource_client
                .delete(&independent_resource)
                .await
                .context(error::DeleteSnafu {
                    what: independent_resource.to_string(),
                })?;

            println!("Deleting resource '{}'", independent_resource);
            resource_client
                .wait_for_deletion(&independent_resource)
                .await;
            println!("Resource '{}' has been deleted", independent_resource);
        }
    }

    Ok(())
}

/// Creates a `TopologicalSort` containing all resources that `test_name` depends on
/// which provides an order for deletion.
async fn resource_deletion_order_for_test(
    k8s_client: Client,
    test_name: &str,
) -> Result<TopologicalSort<String>> {
    let test_client = TestClient::new_from_k8s_client(k8s_client.clone());
    let test = test_client.get(test_name).await.context(error::GetSnafu {
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
        let resource = resource_client
            .get(&resource_name)
            .await
            .context(error::GetSnafu {
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
