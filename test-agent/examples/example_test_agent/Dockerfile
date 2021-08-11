# TODO Use Bottlerocket SDK
FROM rust:1.53.0
WORKDIR /src
ADD ./ /src/
WORKDIR /src/test-agent
RUN cargo install --path . --example example_test_agent --root ./
ENTRYPOINT ["/src/test-agent/bin/example_test_agent"]
