#![cfg(feature = "integ")]
mod data;
use assert_cmd::Command;
use selftest::Cluster;
use tokio::time::Duration;

#[tokio::test]
async fn test_install() {
    let cluster_name = "install-test";
    let max_wait_iter = 25;
    let wait_time = 1000;
    let cluster = Cluster::new(cluster_name).unwrap();
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
    let mut iter_count = 0;
    while !cluster.is_controller_running().await.unwrap() && iter_count < max_wait_iter {
        iter_count += 1;
        tokio::time::sleep(Duration::from_millis(wait_time)).await;
    }
    if iter_count == max_wait_iter {
        panic!(
            "Controller did not reach `running` after {} ms",
            wait_time * max_wait_iter
        )
    }
}

#[tokio::test]
async fn test_run_file() {
    let cluster_name = "run-file-test";
    let max_wait_iter = 25;
    let wait_time = 1000;
    let cluster = Cluster::new(cluster_name).unwrap();
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
        .load_image_to_cluster("example-testsys-agent:integ")
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

    let mut iter_count = 0;
    while !cluster.is_controller_running().await.unwrap() && iter_count < max_wait_iter {
        iter_count += 1;
        tokio::time::sleep(Duration::from_millis(wait_time)).await;
    }
    while !cluster.is_test_running("hello-bones").await.unwrap() && iter_count < max_wait_iter {
        iter_count += 1;
        tokio::time::sleep(Duration::from_millis(wait_time)).await;
    }
    if iter_count == max_wait_iter {
        panic!(
            "Controller or test did not reach `running` after {} ms",
            wait_time * max_wait_iter
        )
    }
}

#[tokio::test]
async fn test_add_file() {
    let cluster_name = "add-file-test";
    let max_wait_iter = 50;
    let wait_time = 1000;
    let cluster = Cluster::new(cluster_name).unwrap();
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

    let mut iter_count = 0;
    while !cluster.is_controller_running().await.unwrap() && iter_count < max_wait_iter {
        iter_count += 1;
        tokio::time::sleep(Duration::from_millis(wait_time)).await;
    }
    // TODO - have an actual resource request and check that it is fulfilled.
    // while !cluster.is_provider_running("robot-provider").await.unwrap()
    //     && iter_count < max_wait_iter
    // {
    //     iter_count += 1;
    //     tokio::time::sleep(Duration::from_millis(wait_time)).await;
    // }
    if iter_count == max_wait_iter {
        panic!(
            "`Controller` or `ResourceProvider` did not reach `running` after {} ms",
            wait_time * max_wait_iter
        )
    }
}

#[tokio::test]
async fn test_status() {
    let cluster_name = "status-test";
    let max_wait_iter = 25;
    let wait_time = 1000;
    let cluster = Cluster::new(cluster_name).unwrap();
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
        .load_image_to_cluster("example-testsys-agent:integ")
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

    let mut iter_count = 0;
    while !cluster.is_controller_running().await.unwrap() && iter_count < max_wait_iter {
        iter_count += 1;
        tokio::time::sleep(Duration::from_millis(wait_time)).await;
    }
    while !cluster.is_test_running("hello-bones").await.unwrap() && iter_count < max_wait_iter {
        iter_count += 1;
        tokio::time::sleep(Duration::from_millis(wait_time)).await;
    }
    if iter_count == max_wait_iter {
        panic!(
            "Controller or test did not reach `running` after {} ms",
            wait_time * max_wait_iter
        )
    }
    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "status",
        "--wait",
    ]);
    cmd.assert().success();
}
