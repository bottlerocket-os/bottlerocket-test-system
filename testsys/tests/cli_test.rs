#![cfg(feature = "integ")]
mod data;
use assert_cmd::Command;
use selftest::Cluster;
use tokio::time::Duration;

const CONTROLLER_TIMEOUT: Duration = Duration::from_secs(60);
const TEST_POD_TIMEOUT: Duration = Duration::from_secs(60);

#[tokio::test]
async fn test_install() {
    let cluster_name = "install-test";
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
    cluster
        .wait_for_controller(CONTROLLER_TIMEOUT)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_run_file() {
    let cluster_name = "run-file-test";
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
    cluster
        .load_image_to_cluster("example-test-agent:integ")
        .unwrap();
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "run",
        "file",
        data::hello_example_path().to_str().unwrap(),
    ]);
    cmd.assert().success();

    cluster
        .wait_for_controller(CONTROLLER_TIMEOUT)
        .await
        .unwrap();

    cluster
        .wait_for_test_pod("hello-bones", TEST_POD_TIMEOUT)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_add_file() {
    let cluster_name = "add-file-test";
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
    cluster
        .load_image_to_cluster("example-resource-agent:integ")
        .unwrap();
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "add",
        "file",
        data::example_resource_provider_path().to_str().unwrap(),
    ]);
    cmd.assert().success();

    cluster
        .wait_for_controller(CONTROLLER_TIMEOUT)
        .await
        .unwrap();
    // TODO - have an actual resource request and check that it is fulfilled.
    // while !cluster.is_provider_running("robot-provider").await.unwrap()
    //     && iter_count < max_wait_iter
    // {
    //     iter_count += 1;
    //     tokio::time::sleep(Duration::from_millis(wait_time)).await;
    // }
}

#[tokio::test]
async fn test_status() {
    let cluster_name = "status-test";
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
    cluster
        .load_image_to_cluster("example-test-agent:integ")
        .unwrap();
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "run",
        "file",
        data::hello_example_path().to_str().unwrap(),
    ]);
    cmd.assert().success();

    cluster
        .wait_for_controller(CONTROLLER_TIMEOUT)
        .await
        .unwrap();

    cluster
        .wait_for_test_pod("hello-bones", TEST_POD_TIMEOUT)
        .await
        .unwrap();

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
async fn test_set() {
    let cluster_name = "set-test";
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
    cluster
        .load_image_to_cluster("example-test-agent:integ")
        .unwrap();
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "run",
        "file",
        data::hello_example_path().to_str().unwrap(),
    ]);
    cmd.assert().success();

    cluster
        .wait_for_controller(CONTROLLER_TIMEOUT)
        .await
        .unwrap();

    cluster
        .wait_for_test_pod("hello-bones", TEST_POD_TIMEOUT)
        .await
        .unwrap();

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "set",
        "hello-bones",
        "--keep-running",
        "false",
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
