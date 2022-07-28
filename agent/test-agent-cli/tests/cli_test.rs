#![cfg(feature = "integ")]
mod data;
use assert_cmd::Command;
use selftest::Cluster;
use tokio::time::Duration;

const POD_TIMEOUT: Duration = Duration::from_secs(300);
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
    cluster.wait_for_controller(POD_TIMEOUT).await.unwrap();

    cluster
        .load_image_to_cluster("example-test-agent-cli:integ")
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
}
