#![cfg(feature = "integ")]
mod data;
use assert_cmd::Command;
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
}
