.PHONY: build sdk-openssl example-test-agent-image controller-image images

ARCH=$(shell uname -m)

images: controller-image

# Builds, Lints and Tests the Rust workspace
build:
	cargo fmt -- --check
	cargo build --locked
	cargo test --locked

# Augment the bottlerocket-sdk image with openssl built with the musl toolchain
sdk-openssl:
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(ARCH)" \
		--tag "bottlerocket-sdk-openssl-$(ARCH)" \
		-f Dockerfile.sdk_openssl .

# Build the container image for the example test-agent program
example-test-agent-image: sdk-openssl
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(ARCH)" \
		--tag "example-testsys-agent" \
		-f agent/test-agent/examples/example_test_agent/Dockerfile .

controller-image: sdk-openssl
	docker build $(DOCKER_BUILD_FLAGS) \
		--build-arg ARCH="$(ARCH)" \
		--tag "testsys-controller" \
		-f controller/Dockerfile .
