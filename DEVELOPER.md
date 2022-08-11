# Developer Guide for TestSys

## What is TestSys

TestSys is a framework that leverages Kubernetes (K8s) to create resources and run tests.
To learn more about how it works, see the [design](design/DESIGN.md) document.

### Navigating the `bottlerocket-test-system` repo

#### [Makefile](Makefile)

Any image can be built using `make <desired image>`.
To make all bottlerocket related images, `make images` can be used, then `make tag-images` can be used to give all images the same tag and `make publish-images` can be used to publish all images.

#### [Dockerfile](Dockerfile)

TestSys relies on Docker images to run tests and create resources.
The Dockerfile contains the logic needed to create all bottlerocket related images.

#### [Agent](agent)

TestSys is broken up into 3 main things: the controller, test agent, and resource agent.

[agent-common](agent/agent-common) contains logic that is related to both the test and resource agents.

[test-agent](agent/test-agent) provides the framework for creating tests.
A test is a program that runs a test and reports its outcome by updating the TestSys Test CRD object's status fields. 
The test agent is packaged into a container image and the controller runs it in a pod as a Kubernetes Job. 
To create a test agent, the [`Runner` trait](agent/test-agent/src/lib.rs) needs to be implemented.
Check out the [example test agent](agent/test-agent/examples/example_test_agent/main.rs) to see how to create a test agent.

[resource-agent](agent/resource-agent) provides the framework for creating resources.
A resource agent can create and destroy external resources (such as Kubernetes clusters or compute instances) that are needed for a test. 
The resource agent program is packaged into a container image and is represented by a Resource CRD object.
A test can then depend on the existence of a resource and the controller will ensure the resource agent runs before the test is run. 
The resource agent is also responsible for destroying a resource after a test runs.
To create a resource agent, the [`Create` trait](agent/resource-agent/src/mod.rs) and [`Destroy` trait](agent/resource-agent/src/mod.rs) need to be implemented.
Check out the [example resource agent](agent/resource-agent/examples/example_resource_agent/main.rs) to see how to create a resource agent.

### [Bottlerocket](bottlerocket)

The bottlerocket directory of the repo is broken into 3 sections: agents, testsys, and types.

[agents](bottlerocket/agents) contains the implementations of all bottlerocket agents.

The types used by the agents can be found in [types](bottlerocket/types).

[testsys](bottlerocket/testsys) contains a CLI implementation that is closely related to bottlerocket agents but will be removed in the near future.

### [Cli](cli)

Cli is an example command line interface that uses the [TestManager](model/src/test_manager/manager.rs) to interact with a TestSys cluster.

### [Controller](controller)

The controller orchestrates the interaction between TestSys tests and resources. 
After a TestSys Test CRD or TestSys Resource CRD has been added to the cluster (`cli run file`), the controller begins the reconciliation process.

For tests, once necessary resources have been created (resources named in the `resources` field of the test spec), a K8s job is created and the test agent is run.

For resources, once necessary resources have been created (resources named in the `depends_on` field of the resource spec), a K8s job is created and the `create` function of the agent is run.
Once a resource is marked for deletion (`cli delete`), the `destroy` function of the resource is run, and the finalizers are removed so that the resource can be cleaned up by K8s.

For more info, see the [design](design/DESIGN.md) document. 

### [Model](model)

The model library contains the infrastructure that TestSys is built upon.
This includes the CRDs for [tests](model/src/test.rs) and [resources](model/src/resource.rs) as well as [everything](model/system) needed to be installed to the TestSys cluster.

All interactions with the TestSys cluster should be done using the model library through `TestManager`, `TestClient`, or `ResourceClient`.

The model provides [TestManager](model/src/test_manager/manager.rs) which can be used as an entry point for user interactions with TestSys.
To see how to use the TestManager look at the [example CLI](cli).

Model also contains a [TestClient](model/src/clients/test_client) for simple interactions with Tests and a [ResourceClient](model/src/clients/resource_client) for Resources.

### [Selftest](selftest)

Selftest is a library used to validate incoming pull request by testing the overall TestSys framework.

## Getting Started

TestSys is currently tested on `x86_64` machines, these steps may not work as expected on other operating systems.

### Setting up TestSys

#### Create a TestSys directory
Create a working directory and navigate to it. 
The example provided creates a directory in the users home, but a directory may be created anywhere.

```bash
cd ~
mkdir testsys && cd testsys
pwd 
```

#### Clone the TestSys repo
While in the TestSys directory, clone the `bottlerocket-test-system` repo from GitHub.

```bash
git clone https://github.com/bottlerocket-os/bottlerocket-test-system.git
cd bottlerocket-test-system
export TESTSYS_DIR=$(pwd)
```

The TestSys directory is now set up.

### Installing tools

#### Install `Kind`
Instead of creating a long-standing EKS cluster for TestSys development, `kind` enables the quick creation and destruction of local clusters that exist within the developers Docker which helps improve the testing experience.
Follow the [installation instructions](https://kind.sigs.k8s.io/docs/user/quick-start/) to set up `kind`.

#### Install `Kubectl`
While all testing user interactions can be done with `cli`, it is helpful to look at test, resource, and pod objects in a K8s cluster with `kubectl`.
Follow the [installation instructions]( https://kubernetes.io/docs/tasks/tools/) to set up `kubectl`.

#### Install Docker
Docker is required for building TestSys agent images and the controller image. If Docker is not installed follow the [installation instructions](https://docs.docker.com/get-docker/).

## Running a test

### Navigate to the TestSys directory
```bash
cd ${TESTSYS_DIR}
```

### Build testing images
First, the images need to be created and tagged so they can be loaded into a `kind` cluster.
```bash
make controller
make example-test-agent
docker tag controller controller:demo
docker tag example-test-agent example-test-agent:demo
```
This step uses the Makefile targets for building the controller image and the example test agent image.
The images then need to be tagged so that the cluster uses loaded images instead of ones from external sources.

### Install the TestSys CLI
Install the `cli` from the repo.
```bash
cargo install --path cli
```

### Create the test yaml file
The yaml file tells the controller and the test agent how the test should be run.
```bash
echo '---
apiVersion: testsys.bottlerocket.aws/v1
kind: Test
metadata:
  name: hello-bones
  namespace: testsys-bottlerocket-aws
spec:
  agent:
    name: hello-agent
    image: "example-test-agent:demo"
    keepRunning: false
    configuration:
      mode: Fast
      person: Bones the Cat
      helloCount: 3
      helloDurationMilliseconds: 500
  resources: []
  dependsOn: []' > example_test_agent.yaml
```

### Create the `kind` cluster
Check if the `kind` cluster already exists.
```bash
kind get clusters
```

If the cluster already exists, it should be deleted.
```bash
kind delete cluster --name testsys-demo
```

Create the new demo cluster.
```bash
kind create cluster --name testsys-demo
```

Now the images created earlier need to be added to the cluster.
```bash
kind load docker-image controller:demo example-test-agent:demo --name testsys-demo
```

### Install TestSys
Before any tests can be run, the K8s cluster needs to understand what a test is and how to handle them. 
To do this TestSys needs to be installed to the cluster.
```bash
cli install --controller-uri controller:demo
```

### Run the test
The CLI installed earlier gives the ability to run tests on a cluster.
```bash
cli run file example_test_agent.yaml
```
This adds the test to the cluster. 
Once the controller sees the test, a test pod will be created that runs the image we provided. 

### Watch the test
The CLI also provides the ability to monitor the test.
If `watch` is available, `watch cli status` will continue to update the status until the test is complete. 
Otherwise `cli status` can be called regularly to see updates.

### Read the test logs
In the event that a test fails, it may be helpful to read through the test logs to see what went wrong.
```bash
cli logs --test hello-bones
```
While debugging tests it may be helpful to monitor logs as they become available, the `--follow` flag is used to print logs that are sent after the initial cli call.
```bash
cli logs --test hello-bones --follow
```

### Cleanup
When creating tests and resources it is important to maintain a clean state because tests must have a unique name in a cluster.
For instance, calling `cli run file example_test_agent.yaml` will cause an error because there is already a test named `hello-bones`.
To delete all tests and resources from the cluster use `cli delete`.
Another option is to remove everything from the TestSys namespace using `cli uninstall`.
This will delete the TestSys namespace and everything inside it, including the controller, secrets, tests and resources.

*Note: If a `kind` cluster is being created and destroyed each time, there is no need to manually delete the tests.*

### Delete the `kind` cluster
```bash
kind delete cluster --name testsys-demo
```

## Running a resource

### Navigate to the TestSys directory
```bash
cd ${TESTSYS_DIR}
```

### Build testing images
First, the images need to be created and tagged so they can be loaded into a `kind` cluster.
```bash
make controller
make example-test-agent
make duplicator-resource-agent
docker tag controller controller:demo
docker tag example-test-agent example-test-agent:demo
docker tag duplicator-resource-agent duplicator-resource-agent:demo
```
This step uses the Makefile targets for building the controller image, the duplicator resource agent image, and the example test agent image. The images then need to be tagged so that the cluster uses loaded images instead of ones from external sources.

### Install the TestSys cli
Install `cli` from the repo. 
*This step can be skipped if the cli is already installed.*
```bash
cargo install --path cli
```

### Create the resource and test yaml file
The yaml file describes 2 CRDs that will be created.
`cli run file` has the ability to create as many TestSys objects at the same time.
Each object should be separated by `---`.

The first is the duplicator resource. 
This resource takes whatever configuration is provided and returns it as a created resource.
In this example `{info: 3}` is the provided configuration.

The test agent is very similar to the previous example except we have added a depended resource.
The resources field of the test CRD signifies that the test should not be run until all resources have finished.
The other change is the hello count. 
In the previous example, it was set to be 3.
In this example, the value is set based on the duplicator resources output.
Any field can be configured to take values from other resources by using `${<resource name>.<field name>}`.
```bash
echo '---
apiVersion: testsys.bottlerocket.aws/v1
kind: Resource
metadata:
  name: duplicator
  namespace: testsys-bottlerocket-aws
spec:
  agent:
    name: dup-agent
    image: "duplicator-resource-agent:demo"
    keepRunning: false
    configuration:
      info: 3
  dependsOn: []
---
apiVersion: testsys.bottlerocket.aws/v1
kind: Test
metadata:
  name: hello-bones
  namespace: testsys-bottlerocket-aws
spec:
  agent:
    name: hello-agent
    image: "example-test-agent:demo"
    keepRunning: false
    configuration:
      mode: Fast
      person: Bones the Cat
      helloCount: ${duplicator.info}
      helloDurationMilliseconds: 500
  resources: [duplicator]
  dependsOn: []' > example.yaml
```

### Create the `kind` cluster
Check if the `kind` cluster already exists.
```bash
kind get clusters
```

If the cluster already exists, it should be deleted.
```bash
kind delete cluster --name testsys-demo
```

Create the new demo cluster.
```bash
kind create cluster --name testsys-demo
```

Now the images created earlier need to be added to the cluster.
```bash
kind load docker-image controller:demo example-test-agent:demo duplicator-resource-agent:demo --name testsys-demo
```

### Install TestSys
Before any tests can be run, the K8s cluster needs to understand what a test is and how to handle them. 
To do this TestSys needs to be installed to the cluster.
```bash
cli install --controller-uri controller:demo
```

### Run the test
The CLI installed earlier gives the ability to run tests on a cluster.
```bash
cli run file example.yaml
```
This adds the test and duplicator resource to the cluster.

### Watch the test and resource
The CLI also provides the ability to monitor the test.
If `watch` is available, `watch cli status` will continue to update the status until the test is complete. 
Otherwise `cli status` can be called regularly to see updates.
Since the test has `duplicator` as a resource, the `duplicator` resource should reach the completed state before the test starts running.

### Read the test logs
In the event that a resource fails, it may be helpful to read through the resource logs to see what went wrong.
The resource state is required to see resource logs because a different pod is used for the resource creation and resource destruction.
```bash
cli logs --resource hello-bones --state Creation
```
While debugging resources it may be helpful to monitor logs as they become available, the `--follow` flag is used to print logs that are sent after the initial cli call.
```bash
cli logs --resource hello-bones --state Creation --follow
```

### Cleanup
When creating tests and resources it is important to maintain a clean state because tests must have a unique name in a cluster.
To delete all tests and resources from the cluster use `cli delete`.
Another option is to remove everything from the TestSys namespace using `cli uninstall`.
This will delete the TestSys namespace and everything inside it, including the controller, secrets, tests and resources.

*Note: If a `kind` cluster is being created and destroyed each time, there is no need to manually delete the tests/resources.*

### Delete the `kind` cluster
```bash
kind delete cluster --name testsys-demo
```
