/*!

The custom resource definitions are modeled as Rust structs in the client crate. Here we generate
the corresponding k8s yaml files. These are needed when setting up a TestSys cluster. Crates that
depend on these files can add yamlgen as a build dependency to ensure the files are current. Scripts
can call `cargo build --package yamlgen`.

!*/

use client::model::{ResourceProvider, Test};
use kube::CustomResourceExt;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

const YAMLGEN_DIR: &str = env!("CARGO_MANIFEST_DIR");
const HEADER: &str = "# This file is generated. Do not edit.\n";

fn main() {
    // Re-run this build script if the model changes.
    println!("cargo:rerun-if-changed=../model/src");

    let path = PathBuf::from(YAMLGEN_DIR)
        .join("deploy")
        .join("testsys.yaml");

    let mut f = File::create(&path).expect(&format!(
        "unable to open file '{}' for writing",
        path.display()
    ));

    f.write(HEADER.as_bytes())
        .expect("unable to write file header");
    serde_yaml::to_writer(&f, &Test::crd()).expect("unable to write Test CRD");
    serde_yaml::to_writer(&f, &ResourceProvider::crd())
        .expect("unable to write ResourceProvider CRD");
}
