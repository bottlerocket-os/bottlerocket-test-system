#![cfg(feature = "integ")]
mod data;
use assert_cmd::Command;
use model::{constants::NAMESPACE, Resource, Test};
use selftest::Cluster;
use tokio::time::Duration;

/// The amount of time we will wait for the controller to run, a test-agent to run, etc. before we
/// consider the selftest a failure. This can be a very long time on resource constrained or
/// machines running a VM for docker.
const POD_TIMEOUT: Duration = Duration::from_secs(300);

const TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// We will test:
/// `testsys install`
/// 'testsys run` (with a manifest)
/// `testsys status`
/// Ensure templating of resources and tests works
/// Ensure depends on for resources and tests works

#[tokio::test]
async fn test_system() {
    let cluster_name = "integ-test";
    let cluster = Cluster::new(cluster_name).unwrap();
    cluster.load_image_to_cluster("controller:integ").unwrap();
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "install",
        "--controller-uri",
        "controller:integ",
    ]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "status",
        "-c",
        "--wait",
    ])
    .timeout(POD_TIMEOUT);
    cmd.assert().success();

    cluster
        .load_image_to_cluster("example-test-agent:integ")
        .unwrap();
    cluster
        .load_image_to_cluster("duplicator-resource-agent:integ")
        .unwrap();

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "run",
        "file",
        data::integ_test_dependent_path().to_str().unwrap(),
    ]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "status",
        "-t",
        "hello-bones-1",
        "-r",
        "dup-1",
        "--wait",
    ])
    .timeout(TEST_TIMEOUT);
    cmd.assert().failure();

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "run",
        "file",
        data::integ_test_depended_on_path().to_str().unwrap(),
    ]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "status",
        "-t",
        "hello-bones-2",
        "--wait",
    ])
    .timeout(POD_TIMEOUT);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "set",
        "hello-bones-1",
        "--keep-running",
        "false",
    ]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "status",
        "-t",
        "hello-bones-1",
        "--wait",
    ])
    .timeout(POD_TIMEOUT);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "status",
        "-t",
        "-r",
        "-c",
        "--wait",
    ])
    .timeout(POD_TIMEOUT);
    cmd.assert().success();

    // Delete everything
    cluster.delete_resource("dup-1").await.unwrap();
    cluster
        .wait_for_resource_destruction_pod("dup-1", POD_TIMEOUT)
        .await
        .unwrap();
    cluster.delete_resource("dup-2").await.unwrap();
    cluster
        .wait_for_resource_destruction_pod("dup-2", POD_TIMEOUT)
        .await
        .unwrap();
    cluster.delete_test("hello-bones-1").await.unwrap();
    cluster.delete_test("hello-bones-2").await.unwrap();
    cluster
        .wait_for_deletion::<Resource>("dup-1", Some(NAMESPACE), POD_TIMEOUT)
        .await
        .unwrap();
    cluster
        .wait_for_deletion::<Resource>("dup-2", Some(NAMESPACE), POD_TIMEOUT)
        .await
        .unwrap();
    cluster
        .wait_for_deletion::<Test>("hello-bones-1", Some(NAMESPACE), POD_TIMEOUT)
        .await
        .unwrap();
    cluster
        .wait_for_deletion::<Test>("hello-bones-2", Some(NAMESPACE), POD_TIMEOUT)
        .await
        .unwrap();

    ////////////////////////////////////////////////////////////////////////////////////////////////
    // Create a resource with destructionPolicy: never and do a best-effort assertion that the
    // destruction pod was not created.
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "run",
        "file",
        data::integ_test_resource_destruction_never_path()
            .to_str()
            .unwrap(),
    ]);
    cmd.assert().success();

    // We need to give k8s some time to finalize the object creation.
    // tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "status",
        "-r",
        "never-destroy",
        "--wait",
    ])
    .timeout(POD_TIMEOUT);
    cmd.assert().success();

    // Watch for a bit and fail if we see the destruction pod.
    for _ in 0..20 {
        assert!(!cluster
            .does_resource_destruction_pod_exist("never-destroy")
            .await
            .unwrap());
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await
    }

    cluster.delete_resource("never-destroy").await.unwrap();
    cluster
        .wait_for_deletion::<Resource>("never-destroy", Some(NAMESPACE), POD_TIMEOUT)
        .await
        .unwrap();
}
