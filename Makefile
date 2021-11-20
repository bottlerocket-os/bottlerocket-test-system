TOP := $(dir $(firstword $(MAKEFILE_LIST)))

.PHONY: build sdk-openssl example-test-agent example-resource-agent controller images sonobuoy-test-agent integ-test ec2-resource-agent eks-resource-agent show-variables

TESTSYS_BUILD_HOST_UNAME_ARCH=$(shell uname -m)
TESTSYS_BUILD_HOST_GOARCH ?= $(lastword $(subst :, ,$(filter $(TESTSYS_BUILD_HOST_UNAME_ARCH):%,x86_64:amd64 aarch64:arm64)))
TESTSYS_BUILD_HOST_PLATFORM=$(shell uname | tr '[:upper:]' '[:lower:]')

export DOCKER_BUILDKIT=1
export CARGO_HOME = $(TOP)/.cargo

show-variables:
	$(info TESTSYS_BUILD_HOST_UNAME_ARCH=$(TESTSYS_BUILD_HOST_UNAME_ARCH))
	$(info TESTSYS_BUILD_HOST_GOARCH=$(TESTSYS_BUILD_HOST_GOARCH))
	$(info TESTSYS_BUILD_HOST_PLATFORM=$(TESTSYS_BUILD_HOST_PLATFORM))
	@echo > /dev/null

# Fetches crates from upstream
fetch:
	cargo fetch --locked

images: fetch controller sonobuoy-test-agent ec2-resource-agent eks-resource-agent

# Builds, Lints and Tests the Rust workspace
build: fetch
	cargo fmt -- --check
	cargo clippy --locked
	cargo build --locked
	cargo test --locked
	cargo test --features integ --no-run

# Build the container image for the example test-agent program
example-test-agent: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--tag "example-test-agent" \
		--network none \
		-f agent/test-agent/examples/example_test_agent/Dockerfile .

# Build the container image for the example resource-agent program
example-resource-agent: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--tag "example-resource-agent" \
		--network none \
		-f agent/resource-agent/examples/example_resource_agent/Dockerfile .

# Build the container image for the example duplicator resource-agent program
duplicator-resource-agent: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--tag "duplicator-resource-agent" \
		--network none \
		-f agent/resource-agent/examples/duplicator_resource_agent/Dockerfile .

# Build the container image for the testsys controller
controller: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--tag "controller" \
		-f controller/Dockerfile .

# Build the container image for a testsys agent
eks-resource-agent ec2-resource-agent sonobuoy-test-agent: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--build-arg GOARCH="$(TESTSYS_BUILD_HOST_GOARCH)" \
		--target $@ \
		--tag $@ \
		.

# If TESTSYS_SELFTEST_SKIP_IMAGE_BUILDS is set to a non-zero-length string, the container images
# will not be rebuilt.
integ-test: export TESTSYS_SELFTEST_KIND_PATH := $(shell pwd)/bin/kind
integ-test: $(if $(TESTSYS_SELFTEST_SKIP_IMAGE_BUILDS), ,controller example-test-agent example-resource-agent sonobuoy-test-agent)
	$(shell pwd)/bin/download-kind.sh --platform $(TESTSYS_BUILD_HOST_PLATFORM) --goarch ${TESTSYS_BUILD_HOST_GOARCH}
	docker tag example-test-agent example-test-agent:integ
	docker tag controller controller:integ
	docker tag example-resource-agent example-resource-agent:integ
	docker tag sonobuoy-test-agent sonobuoy-test-agent:integ
	cargo test --features integ -- --test-threads=1
