#![cfg(feature = "integ")]
mod data;
use assert_cmd::Command;
use selftest::Cluster;
use testsys_model::{constants::NAMESPACE, Test};
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

    cluster
        .wait_for_test_pod("hello-world-cli", POD_TIMEOUT)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(15)).await;

    let mut cmd = Command::cargo_bin("testsys").unwrap();
    cmd.args(&[
        "--kubeconfig",
        cluster.kubeconfig().to_str().unwrap(),
        "status",
        "-c",
        "-r",
        "--json",
    ]);

    let parse: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&cmd.output().unwrap().stdout)).unwrap();

    assert_eq!(
        parse
            .get("results")
            .unwrap()
            .get(1)
            .unwrap()
            .get("state")
            .unwrap(),
        &"passed"
    );

    cluster.delete_test("hello-world-cli").await.unwrap();
    cluster
        .wait_for_deletion::<Test>("hello-world-cli", Some(NAMESPACE), POD_TIMEOUT)
        .await
        .unwrap();
}
