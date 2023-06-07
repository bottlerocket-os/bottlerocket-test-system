# TestSys Workload Tests

This directory contains the source files and artifacts for running TestSys workloads.

## Workload Tests

New workload tests should be added under this subdirectory.
The current list of tests are detailed below.

**hello, testsys**

This simple workload test verifies that the most basic of containers can run on the target host.
See [hello-testsys.yaml](hello-testsys.yaml) for an example configuration.

**NVIDIA smoketests**

This workload test executes various CUDA samples to verify GPU functionality for Bottlerocket hosts running on NVIDIA instances.
See [nvidia-smoke.yaml](nvidia-smoke.yaml) for an example configuration.

---

## Workload Test Requirements

Workload tests are container images that execute one or more application workloads.

The expectation of test authors is to provide a container image with the following:

* Provide a script or binary that will run in a container as part of a Kubernetes pod or ECS task
* Provide a `./run.sh` entry point for the test container, optional arguments may be defined through environment variables
* The shell script may be used directly to perform the test, or be a wrapper to allow executing other binaries or scripts
* Use the `$RESULTS_DIR` environment variable to determine where to place output
* At end of execution, collect test output into a results file `results.tar.gz` placed in `$RESULTS_DIR` with all relevant output
* Write the `${RESULTS_DIR}/results.tar.gz` full path name to a `${RESULTS_DIR}/done` file
* Container exit code determines pass/fail - 0 is considered successful completion

These requirements will make the container compliant as a Sonobuoy plugin when run on Kubernetes.

### Examples

To help illustrate, the following could be used as the `run.sh` entry point script:

```bash
#!/bin/env bash

results_dir="${RESULTS_DIR:-/tmp/results}"
mkdir -p "${results_dir}"

# Example of optional argument passed as environment variable to the container
name="${NAME:-spam}"

saveResults() {
     cd "${results_dir}"
     tar czf results.tar.gz *
     echo "${results_dir}/results.tar.gz" > "${results_dir}/done"
}

# Make sure to always capture results in expected place and format
trap saveResults EXIT

# Run whatever you intest to test using this container
/bin/tests/benchmark --name "${name}" >> "${results_dir}/${test_name}-results" 2>&1
```

The container image would then be defined with this script as its `ENTRYPOINT` and could be run by TestSys using something like:

```yaml
apiVersion: testsys.system/v1
kind: Test
metadata:
  name: spam-workload
  namespace: testsys
spec:
  agent:
    configuration:
      kubeconfigBase64: <Base64 encoded kubeconfig for the test cluster workload runs the tests in>
      plugins:
      - name: spam-workload
        image: example/benchmark:v0.0.3
    image: <your k8s-workload-agent image URI>
    name: workload-test-agent
    keepRunning: true
  resources: {}
```
