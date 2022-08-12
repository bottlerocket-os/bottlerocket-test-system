# Steps to use Test-agent Command Line Interface

`test-agent-cli` lets you write TestSys test using Bash and help in receiving and sending information from/to the TestSys cluster.

## Prerequisites

* kind: https://kind.sigs.k8s.io/docs/user/quick-start/
* Kubectl: https://kubernetes.io/docs/tasks/tools/
* Docker : https://docs.docker.com/get-started/

## Steps to install TestSys

Set the TESTSYS_DIR variable to point to the directory in which you have cloned the project. For example:

```shell
export TESTSYS_DIR="${HOME}/repos/bottlerocket-test-system"
```

Set alias

```shell
alias cli="${TESTSYS_DIR}/.cargo/bin/cli"
```

Install the `cli` command line tool into the local CARGO_HOME as:

```shell
cd "${TESTSYS_DIR}"
cargo install --path "${TESTSYS_DIR}/cli" --force
```

## Steps to create Bash based TestAgent

The following commands can be used to communicate with a TestSys cluster.
Create a bash script like [Example test](../examples/example_test_agent_cli/example-test.sh).

```shell
# Get the configuration details and set the task state running
test-agent-cli init

# Get the number of retires allowed in case of failing tests
test-agent-cli retry-count

# Get the secret value using the secret key
test-agent-cli get-secret secret-key

# Send the result of every test run to test object in Controller
test-agent-cli send-result -o pass -p 1 -f 0 -s 0

# Send any error encountered in test
test-agent-cli send-error error-message

# Mark the test as completed
test-agent-cli terminate --results-dir results_directory
```

Create a [Dockerfile](../examples/example_test_agent_cli/Dockerfile).
Remember to set the ENTRYPOINT to the test Bash script and install the required packages.

Create the docker image.

**Note**: Add a target to the `Makefile` to create the new image.  

```shell
make example-test-agent-cli 
docker image tag example-test-agent-cli example-test-agent-cli:bash
```

Create and tag the controller image.

```shell
make controller 
docker image tag controller controller:bash
```

## Steps to use Bash based TestAgent

Check if the cluster already exists:

```shell
kind get clusters
```

If the cluster already exists, it should be deleted.

```shell
kind delete cluster --name <testsys_cluster_name>
```

Create the new cluster.

```shell
kind create cluster --name <testsys_cluster_name>
```

Now the images created earlier need to be added to the cluster.

```shell
kind load docker-image controller:bash example-test-agent-cli:bash --name <testsys_cluster_name>
```

Install TestSys to the cluster.  

```shell
cli install --controller-uri controller:bash 
```

Create a [yaml file](../tests/data/deploy_test.yaml) for Test.

Run the test.

```shell
cli run file <filename>
```

Check the status of the test.

```shell
cli status -c
```

Cleanup all the resources.

```shell
cli delete
```
