TOP := $(dir $(firstword $(MAKEFILE_LIST)))

.PHONY: build sdk-openssl example-test-agent-image example-resource-agent-image controller-image images sonobuoy-test-agent-image integ-test ec2-resource-agent-image eks-resource-agent-image show-variables

TESTSYS_BUILD_HOST_UNAME_ARCH=$(shell uname -m)
TESTSYS_BUILD_HOST_GOARCH ?= $(lastword $(subst :, ,$(filter $(TESTSYS_BUILD_HOST_UNAME_ARCH):%,x86_64:amd64 aarch64:arm64)))

export DOCKER_BUILDKIT=1
export CARGO_HOME = $(TOP)/.cargo

show-variables:
	$(info TESTSYS_BUILD_HOST_UNAME_ARCH=$(TESTSYS_BUILD_HOST_UNAME_ARCH))
	$(info TESTSYS_BUILD_HOST_GOARCH=$(TESTSYS_BUILD_HOST_GOARCH))
	 @echo > /dev/null

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
example-test-agent-image: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--tag "example-testsys-agent" \
		--network none \
		-f agent/test-agent/examples/example_test_agent/Dockerfile .

# Build the container image for the example resource-agent program
example-resource-agent-image: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--tag "example-resource-agent" \
		--network none \
		-f agent/resource-agent/examples/example_resource_agent/Dockerfile .

duplicator-resource-agent-image: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--tag "duplicator-resource-agent" \
		--network none \
		-f agent/resource-agent/examples/duplicator_resource_agent/Dockerfile .

eks-resource-agent-image: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--tag "eks-resource-agent" \
		-f agent/eks-resource-agent/Dockerfile .

controller-image: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--tag "testsys-controller" \
		-f controller/Dockerfile .

sonobuoy-test-agent-image: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg UNAME_ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--build-arg GOARCH="$(TESTSYS_BUILD_HOST_GOARCH)" \
		--tag "sonobuoy-test-agent" \
		-f agent/sonobuoy-test-agent/Dockerfile .

ec2-resource-agent-image: show-variables fetch
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(TESTSYS_BUILD_HOST_UNAME_ARCH)" \
		--tag "ec2-resource-agent" \
		-f agent/ec2-resource-agent/Dockerfile .

integ-test: controller-image example-test-agent-image example-resource-agent-image sonobuoy-test-agent-image
	docker tag example-testsys-agent example-testsys-agent:integ
	docker tag testsys-controller testsys-controller:integ
	docker tag example-resource-agent example-resource-agent:integ
	docker tag sonobuoy-test-agent sonobuoy-test-agent:integ
	cargo test --features integ
