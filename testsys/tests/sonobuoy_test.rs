#![cfg(feature = "integ")]
#![cfg(test)]
use assert_cmd::Command;
use selftest::Cluster;
use tempfile::TempDir;

#[tokio::test]
async fn run_sonobuoy_test() {
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

#[tokio::test]
async fn results_test() {
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
        "--keep-running",
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
    let results_dir = TempDir::new().unwrap();
    let results_location = results_dir.path().join("results.tar.gz");
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "results",
        "-n",
        "sono-test",
        "--destination",
        results_location.to_str().unwrap(),
    ]);
    cmd.assert().success();

    assert!(std::path::Path::new(&results_location).exists());
}
