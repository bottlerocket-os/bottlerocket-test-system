TOP := $(dir $(firstword $(MAKEFILE_LIST)))

TESTSYS_BUILD_HOST_UNAME_ARCH=$(shell uname -m)
TESTSYS_BUILD_HOST_GOARCH ?= $(lastword $(subst :, ,$(filter $(TESTSYS_BUILD_HOST_UNAME_ARCH):%,x86_64:amd64 aarch64:arm64)))
TESTSYS_BUILD_HOST_PLATFORM=$(shell uname | tr '[:upper:]' '[:lower:]')
# On some hosts we get an x509 certificate error and need to set GOPROXY to "direct"
TESTSYS_BUILD_GOPROXY ?= direct

BOTTLEROCKET_SDK_VERSION = v0.25.1
BOTTLEROCKET_SDK_ARCH = $(TESTSYS_BUILD_HOST_UNAME_ARCH)
BOTTLEROCKET_TOOLS_VERSION ?= v0.1.0

BUILDER_IMAGE = public.ecr.aws/bottlerocket/bottlerocket-sdk-$(BOTTLEROCKET_SDK_ARCH):$(BOTTLEROCKET_SDK_VERSION)
TOOLS_IMAGE ?= public.ecr.aws/bottlerocket/bottlerocket-test-tools:$(BOTTLEROCKET_TOOLS_VERSION)

IMAGES = controller sonobuoy-test-agent ec2-resource-agent eks-resource-agent ecs-resource-agent \
	migration-test-agent vsphere-vm-resource-agent ecs-test-agent

.PHONY: build sdk-openssl example-test-agent example-resource-agent \
	images fetch integ-test show-variables cargo-deny tools $(IMAGES)

export DOCKER_BUILDKIT=1
export CARGO_HOME = $(TOP)/.cargo

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

# Builds, Lints and Tests the Rust workspace
build: fetch
	cargo fmt -- --check
	cargo clippy --locked -- -D warnings
	cargo build --locked
	cargo test --locked
	cargo test --features integ --no-run
	# We've seen cases where this can fail with a version conflict, so we need to make sure it's working
	cargo install --path ./cli --force

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
		--progress=plain \
		./tools

# Build the container image for a testsys agent
eks-resource-agent ec2-resource-agent ecs-resource-agent vsphere-vm-resource-agent sonobuoy-test-agent migration-test-agent ecs-test-agent: show-variables fetch
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
integ-test: export TESTSYS_SELFTEST_KIND_PATH := $(shell pwd)/bin/kind
integ-test: TESTSYS_SELFTEST_THREADS ?= 1
integ-test: $(if $(TESTSYS_SELFTEST_SKIP_IMAGE_BUILDS), ,controller example-test-agent duplicator-resource-agent)
	$(shell pwd)/bin/download-kind.sh --platform $(TESTSYS_BUILD_HOST_PLATFORM) --goarch ${TESTSYS_BUILD_HOST_GOARCH}
	docker tag example-test-agent example-test-agent:integ
	docker tag controller controller:integ
	docker tag duplicator-resource-agent duplicator-resource-agent:integ
	cargo test --features integ -- --test-threads=$(TESTSYS_SELFTEST_THREADS)

cargo-deny:
	# Install cargo-deny to CARGO_HOME which is set to be .cargo in this repository
	cargo install --version 0.9.1 cargo-deny --locked
	cargo fetch
	cargo deny --all-features --no-default-features check --disable-fetch licenses sources
