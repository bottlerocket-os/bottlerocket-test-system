TOP := $(dir $(firstword $(MAKEFILE_LIST)))

.PHONY: build sdk-openssl example-test-agent-image example-resource-agent-image controller-image images sonobuoy-test-agent-image integ-test ec2-resource-agent-image

UNAME_ARCH=$(shell uname -m)
ARCH ?= $(lastword $(subst :, ,$(filter $(UNAME_ARCH):%,x86_64:amd64 aarch64:arm64)))

export DOCKER_BUILDKIT=1
export CARGO_HOME = $(TOP)/.cargo

# Fetches crates from upstream
fetch:
	cargo fetch --locked

images: fetch controller-image

# Builds, Lints and Tests the Rust workspace
build: fetch
	cargo fmt -- --check
	cargo clippy --locked
	cargo build --locked
	cargo test --locked

# Build the container image for the example test-agent program
example-test-agent-image: fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(UNAME_ARCH)" \
		--tag "example-testsys-agent" \
		--network none \
		-f agent/test-agent/examples/example_test_agent/Dockerfile .

# Build the container image for the example resource-agent program
example-resource-agent-image: fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(UNAME_ARCH)" \
		--tag "example-resource-agent" \
		--network none \
		-f agent/resource-agent/examples/example_resource_agent/Dockerfile .

controller-image: fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(UNAME_ARCH)" \
		--tag "testsys-controller" \
		-f controller/Dockerfile .

sonobuoy-test-agent-image: fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg UNAME_ARCH="$(UNAME_ARCH)" \
		--build-arg ARCH="$(ARCH)" \
		--tag "sonobuoy-test-agent" \
		-f agent/sonobuoy-test-agent/Dockerfile .

ec2-resource-agent-image: fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(UNAME_ARCH)" \
		--tag "ec2-resource-agent" \
		-f agent/ec2-resource-agent/Dockerfile .

integ-test: fetch controller-image example-test-agent-image example-resource-agent-image sonobuoy-test-agent-image
	docker tag example-testsys-agent example-testsys-agent:integ
	docker tag testsys-controller testsys-controller:integ
	docker tag example-resource-agent example-resource-agent:integ
	docker tag sonobuoy-test-agent sonobuoy-test-agent:integ
	cargo test --features integ
