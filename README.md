# Bottlerocket Test System

A system for testing Bottlerocket.
To learn more about how it works, see the [design](design/DESIGN.md) document.

## Overview

The system consists of a command line interface, Kubernetes controller, custom resource definition (CRD) objects and containers that allow you to create resources and run tests.
You install TestSys into a cluster of your choice, we call this the *TestSys cluster*.
When running a test, resource agents create an external cluster where we run Bottlerocket instances and run tests.
This is called an *external cluster*.

### Project Status

🚧 👷

The project is in active pre-release development.
Eventually we plan to publish container images and other aspects of the system, but we aren't quite there yet.
We also are not quite ready for external contributions, but we are happy to respond to issues and discussions.

## Quickstart

Since nothing has been published yet, you will have to build everything!
You will need `docker`, `cargo`, `make`, `kind`, and the `aws` and `eksctl` CLIs for this.
Caution: if you follow these instructions, you will create an EKS cluster and EC2 instances!

Set the `TESTSYS_DIR` variable to point to the directory in which you have cloned the project.
For example:

```shell
export TESTSYS_DIR="${HOME}/repos/bottlerocket-test-system"
```

Set a few more variables.
You might want to change the AWS region:

```shell
export TESTSYS_DIR="$HOME/repos/bottlerocket-test-system"
export TESTSYS_CLUSTER_NAME=testsys
export EKS_CLUSTER_NAME=external-cluster
export KUBECONFIG="/tmp/${TESTSYS_CLUSTER_NAME}.yaml"
export EKS_REGION="us-west-2"
export CARGO_HOME="${TESTSYS_DIR}/.cargo"
alias testsys="${TESTSYS_DIR}/.cargo/bin/testsys"
```

Install the `testsys` command line tool into the local CARGO_HOME and build the containers we need:

```shell
cd "${TESTSYS_DIR}"
cargo install --path "${TESTSYS_DIR}/bottlerocket/testsys" --force

make controller
make ec2-resource-agent
make eks-resource-agent
make sonobuoy-test-agent

docker tag controller controller:eks
docker tag ec2-resource-agent ec2-resource-agent:eks
docker tag eks-resource-agent eks-resource-agent:eks
docker tag sonobuoy-test-agent sonobuoy-test-agent:eks
```

We will use a local `kind` cluster as our TestSys cluster.
Here we create it and load our container images into it using `kind`.
Note, the `kind load docker-image` command frequently reports an error even when everything seems to have worked.

```shell
kind create cluster --name "${TESTSYS_CLUSTER_NAME}"

kind load docker-image \
  controller:eks \
  ec2-resource-agent:eks \
  eks-resource-agent:eks \
  sonobuoy-test-agent:eks \
  --name "${TESTSYS_CLUSTER_NAME}"
```

Next we install the TestSys namespace, controller and CRD schemas into the TestSys cluster.
We also set our kubeconfig context to the testsys namespace for convenience.

```shell
testsys install --controller-uri controller:eks
kubectl config set-context --current --namespace="testsys-bottlerocket-aws"
```

We will be creating an EKS cluster and EC2 instances, so we need to create a Kubernetes secret with our AWS credentials.

```shell
testsys add secret map  \
 --name "aws-creds" \
 "access-key-id=$(aws configure get default.aws_access_key_id)" \
 "secret-access-key=$(aws configure get default.aws_secret_access_key)"
```

Now we are ready to run a Bottlerocket test.
We get the latest AMI ID.
Then we pass it to TestSys which will create an EKS cluster, launch Bottlerocket nodes and run a Sonobuoy test in 'quick' mode.

**Caution**: The command below specifies `never` as the `--cluster-destruction-policy`.
This is because creating a cluster takes a long time, and we might want to re-use it.
To delete the cluster manually, use `eksctl delete cluster "external-cluster"` or delete the relevant CloudFormation stacks.
You can also change the `--cluster-destruction-policy` to `onDeletion` in the command below.
If you do, then when you `kubectl delete resource external-cluster`, the EKS cluster will be deleted.

```shell
K8S_VER="1.21"
REGION="us-west-2"
ARCH="x86_64"
VARIANT="aws-k8s-${K8S_VER}"
export AMI_ID=$(aws ssm get-parameter \
  --region "${REGION}" \
  --name "/aws/service/bottlerocket/${VARIANT}/${ARCH}/latest/image_id" \
  --query Parameter.Value --output text)

testsys run aws-k8s \
  --name "testsys-demo" \
  --test-agent-image "sonobuoy-test-agent:eks" \
  --keep-running \
  --sonobuoy-mode "quick" \
  --aws-secret "aws-creds" \
  --region "${REGION}" \
  --cluster-name "${EKS_CLUSTER_NAME}" \
  --cluster-creation-policy "ifNotExists" \
  --cluster-destruction-policy "never" \
  --cluster-provider-image "eks-resource-agent:eks" \
  --ami "${AMI_ID}" \
  --ec2-provider-image "ec2-resource-agent:eks"
```

### Test Results

Executing the `testsys run aws-k8s` command will kick off the setup and execution of the Sonobuoy tests on an EKS cluster using Bottlerocket as the host OS for the cluster nodes.

To get the status and results of test execution, run the `testsys status` command for a high level summary:

```shell
 NAME           TYPE   STATE    PASSED   SKIPPED   FAILED
 testsys-demo   Test   passed   1        5772      0
```

**Note:** run `testsys status --help` to learn about more options.
Notably the `-c` and `-r` arguments for getting controller and resource status.

When the test run has completed you may get the full logs of the test results.
The content of the resulting tar file will vary depending on the tests being run.

```shell
testsys results --destination testresults.tar --test-name testsys-demo
tar -xvf testresults.tar
```

### Cleanup

If you used the `--cluster-destruction-policy never` as given above, there will be an EKS cluster running at the end of execution.
This can be very convenient to keep around for subsequent test runs.

It does however consume resources and incur usage charges.
If you are done running tests and would like to clean up, run the following to delete the EKS cluster.

```shell
eksctl delete cluster external-cluster
```

**Note:** This will destroy the EKS cluster, so ensure you are no longer
using it and you have the correct cluster name before running the
example command.

## Development

See [DEVELOPER](DEVELOPER.md) for an introduction to the framework and start up guide.

### Project Structure

- `model` is the root dependency. It includes the CRDs and clients for interacting with them.
- `controller` contains the Kubernetes controller responsible for running resource and test pods.
- `agent` contains libraries with the traits and harnesses for creating test and resource agents.
- `bottlerocket/agents` contains the implementations of the test and resource traits that we use for Bottlerocket testing.
- `bottlerocket/testsys` contains the command line interface for installing the system and running tests.

The `model`, `agents` and `controller` crates are general-purpose, and define the TestSys system.
It is possible to use these libraries and controller for testing purposes other than Bottlerocket.

The `bottlerocket/testsys` CLI and `bottlerocket/agents` crates are more specialized to Bottlerocket's testing use cases.

## Security

See [CONTRIBUTING](CONTRIBUTING.md#security-issue-notifications) for more information.

## License

This project is dual licensed under either the Apache-2.0 License or the MIT license, your choice.
