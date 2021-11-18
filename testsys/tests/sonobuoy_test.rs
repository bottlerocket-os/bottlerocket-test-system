#![cfg(feature = "integ")]
use assert_cmd::Command;
use selftest::Cluster;
use tokio::time::Duration;

/// This test requires an external k8s cluster whose kubeconfig must be added to
/// `test_kubeconfig`.
#[tokio::test]
// FIXME - remove assumptions about an existing cluster?
#[ignore]
async fn run_sonobuoy_test() {
    tokio::time::timeout(Duration::from_secs(120), run_sonobuoy_test_impl())
        .await
        .expect("Timeout waiting for run_sonobuoy_test_impl");
}

async fn run_sonobuoy_test_impl() {
    let test_cluster = Cluster::new("sono-test").unwrap();
    let cluster = Cluster::new("sonobuoy-test").unwrap();
    cluster
        .load_image_to_cluster("testsys-controller:integ")
        .unwrap();
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "install",
        "--controller-uri",
        "testsys-controller:integ",
    ]);
    cmd.assert().success();
    cluster
        .load_image_to_cluster("sonobuoy-test-agent:integ")
        .unwrap();
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "run",
        "sonobuoy",
        "--name",
        "sono-test",
        "--image",
        "sonobuoy-test-agent:integ",
        "--target-cluster-kubeconfig",
        test_cluster
            .get_internal_kubeconfig()
            .unwrap()
            .to_str()
            .unwrap(),
        "--plugin",
        "e2e",
        "--mode",
        "quick",
        "--kubernetes-version",
        "v1.21.2",
    ]);
    cmd.assert().success();
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "status",
        "--wait",
    ]);
    cmd.assert().success();
}
