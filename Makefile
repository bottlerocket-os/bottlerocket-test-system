.PHONY: build sdk-openssl example-test-agent-image example-resource-agent-image controller-image images sonobuoy-test-agent-image integ-test

UNAME_ARCH=$(shell uname -m)
ARCH ?= $(lastword $(subst :, ,$(filter $(UNAME_ARCH):%,x86_64:amd64 aarch64:arm64)))

images: controller-image

# Builds, Lints and Tests the Rust workspace
build:
	cargo fmt -- --check
	cargo build --locked
	cargo test --locked

# Augment the bottlerocket-sdk image with openssl built with the musl toolchain
sdk-openssl:
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(UNAME_ARCH)" \
		--tag "bottlerocket-sdk-openssl-$(UNAME_ARCH)" \
		-f Dockerfile.sdk_openssl .

# Build the container image for the example test-agent program
example-test-agent-image: sdk-openssl
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(UNAME_ARCH)" \
		--tag "example-testsys-agent" \
		-f agent/test-agent/examples/example_test_agent/Dockerfile .

# Build the container image for the example resource-agent program
example-resource-agent-image: sdk-openssl
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(UNAME_ARCH)" \
		--tag "example-resource-agent" \
		-f agent/resource-agent/examples/example_resource_agent/Dockerfile .

controller-image: sdk-openssl
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(UNAME_ARCH)" \
		--tag "testsys-controller" \
		-f controller/Dockerfile .

sonobuoy-test-agent-image: sdk-openssl
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg UNAME_ARCH="$(UNAME_ARCH)" \
		--build-arg ARCH="$(ARCH)" \
		--tag "sonobuoy-test-agent" \
		-f agent/sonobuoy-test-agent/Dockerfile .

integ-test: controller-image example-test-agent-image
	docker tag example-testsys-agent example-testsys-agent:integ
	docker tag testsys-controller testsys-controller:integ
	cargo test --features integ
