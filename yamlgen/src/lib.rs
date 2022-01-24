/*!

This crate is used to write out the YAML representation of the TestSys CRDs and other necessary
Kubernetes manifests.
These constructs are defined in Rust and we typically install them using the `testsys` CLI, thus
the YAML representations are not strictly necessary.
These manifests do come in handy though for reference, testing and development.

This `lib.rs` file is intentionally empty as `yamlgen` provides a `build.rs` that is invoked during
builds of other crates that specify `yamlgen` as a `build-dependency`.

!*/
