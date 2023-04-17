# Set our default to print out help if user has not provided a target
.DEFAULT_GOAL := help

# Determine our root directory path for later use
TOP := $(dir $(firstword $(MAKEFILE_LIST)))

# Variables we update as newer versions are released
BOTTLEROCKET_SDK_VERSION = v0.30.2
BOTTLEROCKET_SDK_ARCH = $(TESTSYS_BUILD_HOST_UNAME_ARCH)
BOTTLEROCKET_TOOLS_VERSION ?= v0.4.0

BUILDER_IMAGE = public.ecr.aws/bottlerocket/bottlerocket-sdk-$(BOTTLEROCKET_SDK_ARCH):$(BOTTLEROCKET_SDK_VERSION)
TOOLS_IMAGE ?= public.ecr.aws/bottlerocket-test-system/bottlerocket-test-tools:$(BOTTLEROCKET_TOOLS_VERSION)

# Capture information about the build host
TESTSYS_BUILD_HOST_UNAME_ARCH=$(shell uname -m)
TESTSYS_BUILD_HOST_GOARCH ?= $(lastword $(subst :, ,$(filter $(TESTSYS_BUILD_HOST_UNAME_ARCH):%,x86_64:amd64 aarch64:arm64)))
TESTSYS_BUILD_HOST_PLATFORM=$(shell uname | tr '[:upper:]' '[:lower:]')
# On some hosts we get an x509 certificate error and need to set GOPROXY to "direct"
TESTSYS_BUILD_GOPROXY ?= direct

# The set of agent images. Add new agent artifacts here when added to the
# project
AGENT_IMAGES = sonobuoy-test-agent ec2-resource-agent eks-resource-agent ecs-resource-agent \
               migration-test-agent vsphere-vm-resource-agent vsphere-k8s-cluster-resource-agent \
               ecs-test-agent k8s-workload-agent ecs-workload-agent metal-k8s-cluster-resource-agent

# The set of container images. Add additional artifacts here when added
# to the project
IMAGES = controller $(AGENT_IMAGES)

# Store targets for tagging images
TAG_IMAGES = $(addprefix tag-, $(IMAGES))

# Store targets to push images
PUSH_IMAGES = $(addprefix push-, $(IMAGES))

.PHONY: build example-test-agent example-test-agent-cli example-resource-agent \
	images fetch integ-test show-variables cargo-deny tools $(IMAGES) \
	tag-images $(TAG_IMAGES) push-images $(PUSH_IMAGES) print-image-names \
	help

export DOCKER_BUILDKIT=1
export CARGO_HOME = $(TOP)/.cargo

help: ## display help
	@awk 'BEGIN {FS = ":.* ## "; printf "\n\033[1;32mTargets:\033[36m\033[0m\n"} /^[a-zA-Z_-]+:.*? ## / { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

show-variables:
	$(info TESTSYS_BUILD_HOST_UNAME_ARCH=$(TESTSYS_BUILD_HOST_UNAME_ARCH))
	$(info TESTSYS_BUILD_HOST_GOARCH=$(TESTSYS_BUILD_HOST_GOARCH))
	$(info TESTSYS_BUILD_HOST_PLATFORM=$(TESTSYS_BUILD_HOST_PLATFORM))
	$(info TESTSYS_BUILD_GOPROXY=$(TESTSYS_BUILD_GOPROXY))
	$(info BUILDER_IMAGE=$(BUILDER_IMAGE))
	$(info TOOLS_IMAGE=$(TOOLS_IMAGE))
	@echo > /dev/null

# Fetches crates from upstream
fetch:
	cargo fetch --locked

images: fetch $(IMAGES)

# This target prints the image names. It may be useful to write loops over the names of the containers. For example, we
# might want to check to make sure a repository exists for each image (i.e. maybe a new container has been added to the
# list since we last pushed, so maybe we need to create a new repository).
#
# Example:
# declare -a arr=($(make print-image-names))
# for image_name in "${arr[@]}"
# do
#   echo "do something with $image_name"
# done
print-image-names:
	$(info $(IMAGES))
	@echo > /dev/null

build: fetch  ## build, lint, and test the Rust workspace
	cargo fmt -- --check
	cargo clippy --locked -- -D warnings
	cargo build --locked
	cargo test --locked
	cargo test --features integ --no-run
	# We've seen cases where this can fail with a version conflict, so we need to make sure it's working
	cargo install --path ./cli --force --locked

# Build the container image for the example test-agent program
example-test-agent: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--build-arg BUILDER_IMAGE="$(BUILDER_IMAGE)" \
		--tag "example-test-agent" \
		--network none \
		-f agent/test-agent/examples/example_test_agent/Dockerfile .

# Build the container image for the example resource-agent program
example-resource-agent: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--build-arg BUILDER_IMAGE="$(BUILDER_IMAGE)" \
		--tag "example-resource-agent" \
		--network none \
		-f agent/resource-agent/examples/example_resource_agent/Dockerfile .

# Build the container image for the example test-agent-cli program
example-test-agent-cli: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--build-arg BUILDER_IMAGE="$(BUILDER_IMAGE)" \
		--tag "example-test-agent-cli" \
		-f agent/test-agent-cli/examples/example_test_agent_cli/Dockerfile .

# Build the container image for the example duplicator resource-agent program
duplicator-resource-agent: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--build-arg BUILDER_IMAGE="$(BUILDER_IMAGE)" \
		--tag "duplicator-resource-agent" \
		--network none \
		-f agent/resource-agent/examples/duplicator_resource_agent/Dockerfile .

# Build the container image for the testsys controller
controller: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--build-arg BUILDER_IMAGE="$(BUILDER_IMAGE)" \
		--build-arg TOOLS_IMAGE="$(TOOLS_IMAGE)" \
		--tag "controller" \
		-f controller/Dockerfile .

# Build the 3rd-party tools that we use in our agent containers.
tools:
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--build-arg BUILDER_IMAGE="$(BUILDER_IMAGE)" \
		--build-arg GOARCH="$(TESTSYS_BUILD_HOST_GOARCH)" \
		--build-arg GOPROXY="$(TESTSYS_BUILD_GOPROXY)" \
		--network=host \
		-f ./tools/Dockerfile \
		-t bottlerocket-test-tools \
		-t $(TOOLS_IMAGE) \
		--progress=tty \
		./tools

# Build the container image for a testsys agent
$(AGENT_IMAGES): show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--build-arg BUILDER_IMAGE="$(BUILDER_IMAGE)" \
		--build-arg TOOLS_IMAGE="$(TOOLS_IMAGE)" \
		--build-arg GOARCH="$(TESTSYS_BUILD_HOST_GOARCH)" \
		--build-arg GOPROXY="$(TESTSYS_BUILD_GOPROXY)" \
		--network=host \
		--target $@ \
		--tag $@ \
		--progress=plain \
		.

# TESTSYS_SELFTEST_SKIP_IMAGE_BUILDS - If this is set to a non-zero-length string, the container images will will be
#                                      expected to already exist and will not be built.
# TESTSYS_SELFTEST_THREADS           - The number of tests that cargo will run in parallel. This defaults to 1 since the
#                                      integration tests run Kubernetes clusters in kind which can be resource-intensive
#                                      for some machines.
integ-test:  ## run integration tests
integ-test: export TESTSYS_SELFTEST_KIND_PATH := $(shell pwd)/bin/kind
integ-test: TESTSYS_SELFTEST_THREADS ?= 1
integ-test: $(if $(TESTSYS_SELFTEST_SKIP_IMAGE_BUILDS), ,controller example-test-agent duplicator-resource-agent)
	$(shell pwd)/bin/download-kind.sh --platform $(TESTSYS_BUILD_HOST_PLATFORM) --goarch ${TESTSYS_BUILD_HOST_GOARCH}
	docker tag example-test-agent example-test-agent:integ
	docker tag controller controller:integ
	docker tag example-test-agent-cli  example-test-agent-cli:integ
	docker tag duplicator-resource-agent duplicator-resource-agent:integ
	cargo test --features integ -- --test-threads=$(TESTSYS_SELFTEST_THREADS)

cargo-deny:
	# Install cargo-deny to CARGO_HOME which is set to be .cargo in this repository
	cargo install --version 0.9.1 cargo-deny --locked
	cargo fetch
	cargo deny --all-features --no-default-features check --disable-fetch licenses sources

# Define a target to tag all images
tag-images: $(TAG_IMAGES)  ## tag all images

# This defines the TAG_IMAGE variable, extracting the image name from the target name
$(TAG_IMAGES): TAG_IMAGE = $(@:tag-%=%)
$(TAG_IMAGES): check-publish-version check-single-publish-repository
ifeq ($(SINGLE_IMAGE_REPO), true)
	docker tag $(TAG_IMAGE) $(if $(PUBLISH_IMAGES_REGISTRY), $(PUBLISH_IMAGES_REGISTRY)/)$(PUBLISH_IMAGES_REPO):$(TAG_IMAGE)-$(PUBLISH_IMAGES_VERSION)
else
	docker tag $(TAG_IMAGE) $(if $(PUBLISH_IMAGES_REGISTRY), $(PUBLISH_IMAGES_REGISTRY)/)$(TAG_IMAGE):$(PUBLISH_IMAGES_VERSION)
endif

# Define a target to publish all images
publish-images: $(PUSH_IMAGES)  ## publish all images

# This defines the TAG_IMAGE variable, extracting the image name from the target name
$(PUSH_IMAGES): TAG_IMAGE = $(@:push-%=%)
$(PUSH_IMAGES): check-publish-version check-publish-registry check-single-publish-repository
ifeq ($(SINGLE_IMAGE_REPO), true)
	docker push $(if $(PUBLISH_IMAGES_REGISTRY), $(PUBLISH_IMAGES_REGISTRY)/)$(PUBLISH_IMAGES_REPO):$(TAG_IMAGE)-$(PUBLISH_IMAGES_VERSION)
else
	docker push $(if $(PUBLISH_IMAGES_REGISTRY), $(PUBLISH_IMAGES_REGISTRY)/)$(TAG_IMAGE):$(PUBLISH_IMAGES_VERSION)
endif

check-publish-version:
ifndef PUBLISH_IMAGES_VERSION
	$(error PUBLISH_IMAGES_VERSION is undefined)
endif

check-publish-registry:
ifndef PUBLISH_IMAGES_REGISTRY
	$(error PUBLISH_IMAGES_REGISTRY is undefined)
endif

check-single-publish-repository:
ifdef PUBLISH_IMAGES_REPO
else ifeq ($(SINGLE_IMAGE_REPO), true)
	$(error PUBLISH_IMAGES_REPO is undefined)
endif

mdlint:
	docker run --rm -v "$$(pwd)":/workdir ghcr.io/igorshubovych/markdownlint-cli:latest "**/*.md"
