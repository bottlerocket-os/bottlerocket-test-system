use super::{error, Result, TestManager};
use crate::clients::{AllowNotFound, CrdClient, ResourceClient, TestClient};
use crate::{Crd, CrdName, TaskState};
use futures::channel::mpsc::{channel, Sender};
use futures::executor::block_on;
use futures::{SinkExt, Stream};
use kube::{core::object::HasStatus, ResourceExt};
use snafu::ResultExt;
use std::time::Duration;
use topological_sort::TopologicalSort;

#[derive(Debug)]
pub enum DeleteEvent {
    Starting(CrdName),
    Deleted(CrdName),
    Failed(CrdName),
}

impl TestManager {
    /// Return a stream containing `DeleteEvent` for each object in `deletion_order` that is
    /// deleted.
    pub(super) fn delete_sorted_resources(
        &self,
        mut deletion_order: TopologicalSort<CrdName>,
    ) -> impl Stream<Item = Result<DeleteEvent>> {
        let (mut tx, rx) = channel(100);
        let test_client = self.test_client();
        let resource_client = self.resource_client();
        // Delete our sorted resources
        tokio::task::spawn(async move {
            if let Err(e) =
                async_deletion(&mut tx, &mut deletion_order, test_client, resource_client).await
            {
                if let Err(e) = block_on(tx.send(Err(e))) {
                    eprintln!("Deletion error failed to send: {}", e);
                }
            }
            tx.close_channel();
        });

        rx
    }

    /// Creates a `TopologicalSort` containing all objects in a testsys cluster.
    pub(super) async fn all_objects_deletion_order(&self) -> Result<TopologicalSort<CrdName>> {
        let mut topo_sort = TopologicalSort::new();
        let resource_client = self.resource_client();
        let resources = resource_client
            .get_all()
            .await
            .context(error::ClientSnafu {
                action: "get all resources",
            })?;
        for resource in resources {
            topo_sort.insert(CrdName::Resource(resource.name_any()));
            if let Some(depended_resources) = &resource.spec.depends_on {
                for depended_resource in depended_resources {
                    topo_sort.add_dependency(
                        CrdName::Resource(resource.name_any()),
                        CrdName::Resource(depended_resource.clone()),
                    );
                }
            }
        }
        let test_client = self.test_client();
        let tests = test_client.get_all().await.context(error::ClientSnafu {
            action: "get all tests",
        })?;
        for test in tests {
            if test.spec.resources.is_empty() {
                topo_sort.insert(CrdName::Test(test.name_any()));
            } else {
                for resource in &test.spec.resources {
                    topo_sort.add_dependency(
                        CrdName::Test(test.name_any()),
                        CrdName::Resource(resource.clone()),
                    );
                }
            }
        }
        Ok(topo_sort)
    }

    /// Takes all objects and adds them to a `TopologicalSort` with their dependencies linked.
    pub(super) fn vec_to_deletion_order(objects: Vec<Crd>) -> TopologicalSort<CrdName> {
        let mut topo_sort = TopologicalSort::new();
        let object_names: Vec<CrdName> =
            objects.clone().into_iter().map(|crd| crd.into()).collect();
        for crd in &objects {
            match crd {
                Crd::Test(test) => {
                    let test_crd_name = CrdName::Test(test.name_any());
                    topo_sort.insert(test_crd_name.clone());
                    for resource in &test.spec.resources {
                        let dep_name = CrdName::Resource(resource.to_string());
                        // Make sure that we want the resource to be deleted
                        if object_names.contains(&dep_name) {
                            topo_sort.add_dependency(test_crd_name.clone(), dep_name);
                        }
                    }
                }
                Crd::Resource(resource) => {
                    let resource_crd_name = CrdName::Resource(resource.name_any());
                    topo_sort.insert(resource_crd_name.clone());
                    if let Some(resources) = &resource.spec.depends_on {
                        for resource in resources {
                            let dep_name = CrdName::Resource(resource.to_string());
                            // Make sure that we want the resource to be deleted
                            if object_names.contains(&dep_name) {
                                topo_sort.add_dependency(resource_crd_name.clone(), dep_name);
                            }
                        }
                    }
                }
            }
        }
        topo_sort
    }
}

/// Asyncronously delete all objects in `deletion_order` and send each `DeleteEvent` to `tx`.
async fn async_deletion(
    tx: &mut Sender<Result<DeleteEvent>>,
    deletion_order: &mut TopologicalSort<CrdName>,
    test_client: TestClient,
    resource_client: ResourceClient,
) -> Result<()> {
    let mut awaiting_deletion = Vec::<CrdName>::new();
    loop {
        // Check for all resources that have been deleted for completion.
        let mut still_awaiting = Vec::new();
        for object_name in &awaiting_deletion {
            match object_name {
                CrdName::Test(test_name) => {
                    let test = test_client
                        .get(test_name)
                        .await
                        .allow_not_found(|_| ())
                        .context(error::ClientSnafu {
                            action: format!("get '{}'", test_name),
                        })?;
                    if test.is_some() {
                        still_awaiting.push(CrdName::Test(test_name.to_string()));
                    } else {
                        tx.send(Ok(DeleteEvent::Deleted(CrdName::Test(
                            test_name.to_string(),
                        ))))
                        .await
                        .context(error::SenderSnafu)?;
                    }
                }
                CrdName::Resource(resource_name) => {
                    let resource = resource_client
                        .get(resource_name)
                        .await
                        .allow_not_found(|_| ())
                        .context(error::ClientSnafu {
                            action: format!("get '{}'", resource_name),
                        })?;
                    if let Some(resource) = resource {
                        // If the resource errored during deletion alert the user that a problem
                        // occured
                        if resource
                            .status()
                            .map(|status| status.destruction.task_state == TaskState::Error)
                            .unwrap_or_default()
                        {
                            tx.send(Ok(DeleteEvent::Failed(CrdName::Resource(
                                resource_name.to_string(),
                            ))))
                            .await
                            .context(error::SenderSnafu)?;
                        } else {
                            still_awaiting.push(CrdName::Resource(resource_name.to_string()));
                        }
                    } else {
                        tx.send(Ok(DeleteEvent::Deleted(CrdName::Resource(
                            resource_name.to_string(),
                        ))))
                        .await
                        .context(error::SenderSnafu)?;
                    }
                }
            };
        }
        awaiting_deletion = still_awaiting;

        // If all resources awaiting deletion have been deleted we can get a new set of resources to
        // delete.
        if awaiting_deletion.is_empty() {
            if deletion_order.is_empty() {
                return Ok(());
            }
            awaiting_deletion = deletion_order.pop_all();
            for object in &awaiting_deletion {
                tx.send(Ok(DeleteEvent::Starting(object.clone())))
                    .await
                    .context(error::SenderSnafu)?;
                match object {
                    CrdName::Test(test_name) => test_client
                        .delete(test_name)
                        .await
                        .allow_not_found(|_| ())
                        .context(error::ClientSnafu {
                            action: format!("delete '{}'", test_name),
                        })
                        .map(|_| ()),
                    CrdName::Resource(resource_name) => resource_client
                        .delete(resource_name)
                        .await
                        .allow_not_found(|_| ())
                        .context(error::ClientSnafu {
                            action: format!("delete '{}'", resource_name),
                        })
                        .map(|_| ()),
                }?
            }
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}
