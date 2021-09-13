/*!

The custom resource definitions are modeled as Rust structs in the client crate. Here we generate
the corresponding k8s yaml files. These are needed when setting up a TestSys cluster. Crates that
depend on these files can add yamlgen as a build dependency to ensure the files are current. Scripts
can call `cargo build --package yamlgen`.

!*/

use kube::CustomResourceExt;
use model::model::{ResourceProvider, Test};
use model::system::{
    agent_cluster_role, agent_cluster_role_binding, agent_service_account, controller_cluster_role,
    controller_cluster_role_binding, controller_deployment, controller_service_account,
    testsys_namespace,
};
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

const YAMLGEN_DIR: &str = env!("CARGO_MANIFEST_DIR");
const HEADER: &str = "# This file is generated. Do not edit.\n";
// FIXME: set this to an public ECR image eventually
const DEFAULT_TESTSYS_CONTROLLER_IMAGE: &str =
    "6456745674567.dkr.ecr.us-west-2.amazonaws.com/controller:v0.1.2";

fn main() {
    // Re-run this build script if the model changes.
    println!("cargo:rerun-if-changed=../client/src/model");
    println!("cargo:rerun-if-changed=../client/src/system");
    // Re-run the yaml generation if these variables change
    println!("cargo:rerun-if-env-changed=TESTSYS_CONTROLLER_IMAGE");
    println!("cargo:rerun-if-env-changed=TESTSYS_CONTROLLER_IMAGE_PULL_SECRET");

    let path = PathBuf::from(YAMLGEN_DIR)
        .join("deploy")
        .join("testsys-crd.yaml");
    let mut testsys_crd = File::create(&path).unwrap();

    let path = PathBuf::from(YAMLGEN_DIR)
        .join("deploy")
        .join("testsys-controller.yaml");
    let mut testsys_controller = File::create(&path).unwrap();

    let path = PathBuf::from(YAMLGEN_DIR)
        .join("deploy")
        .join("testsys-agent.yaml");
    let testsys_agent = File::create(&path).unwrap();

    // testsys-crd related K8S manifest
    testsys_crd.write_all(HEADER.as_bytes()).unwrap();
    serde_yaml::to_writer(&testsys_crd, &Test::crd()).unwrap();
    serde_yaml::to_writer(&testsys_crd, &ResourceProvider::crd()).unwrap();

    // Read the controller image and image-pull-secrets identifier from environment variables
    let controller_image = env::var("TESTSYS_CONTROLLER_IMAGE")
        .ok()
        .unwrap_or_else(|| DEFAULT_TESTSYS_CONTROLLER_IMAGE.to_string());
    let controller_image_pull_secrets = env::var("TESTSYS_CONTROLLER_IMAGE_PULL_SECRET").ok();

    // testsys-controller related K8S manifest
    testsys_controller.write_all(HEADER.as_bytes()).unwrap();
    serde_yaml::to_writer(&testsys_controller, &testsys_namespace()).unwrap();
    serde_yaml::to_writer(&testsys_controller, &controller_service_account()).unwrap();
    serde_yaml::to_writer(&testsys_controller, &controller_cluster_role()).unwrap();
    serde_yaml::to_writer(&testsys_controller, &controller_cluster_role_binding()).unwrap();
    serde_yaml::to_writer(
        &testsys_controller,
        &controller_deployment(controller_image, controller_image_pull_secrets),
    )
    .unwrap();

    // testsys-agent related K8S manifest
    serde_yaml::to_writer(&testsys_agent, &testsys_namespace()).unwrap();
    serde_yaml::to_writer(&testsys_agent, &agent_service_account()).unwrap();
    serde_yaml::to_writer(&testsys_agent, &agent_cluster_role()).unwrap();
    serde_yaml::to_writer(&testsys_agent, &agent_cluster_role_binding()).unwrap();
}
