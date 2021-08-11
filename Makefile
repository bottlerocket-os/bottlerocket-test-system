.PHONY: example-test-agent-container

# Build a container image for daemon and tools.
example-test-agent-container:
	docker build \
		--network=host \
		--tag 'example_test_agent' \
		-f test-agent/examples/example_test_agent/Dockerfile .
