# syntax=docker/dockerfile:1.1.3-experimental
ARG BUILDER_IMAGE
FROM ${BUILDER_IMAGE} as build

ARG ARCH
USER root
# We need these environment variables set for building the `openssl-sys` crate
ENV PKG_CONFIG_PATH=/${ARCH}-bottlerocket-linux-musl/sys-root/usr/lib/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV CARGO_HOME=/src/.cargo
ENV OPENSSL_STATIC=true
ENV CARGO_HOME=/src/.cargo
ADD ./ /src/
WORKDIR /src/agent/test-agent-cli
RUN --mount=type=cache,mode=0777,target=/src/target \
    cargo install --locked --offline --target ${ARCH}-bottlerocket-linux-musl --path .  --root ./

FROM public.ecr.aws/amazonlinux/amazonlinux:2
# Install all the required packages for Bash test script.
RUN yum update -y \
    && yum install -y jq \
    && yum clean all

COPY --from=build /src/agent/test-agent-cli/bin/test-agent-cli /usr/local/bin/
# Copy the Bash test script
COPY --from=build /src/agent/test-agent-cli/examples/example_test_agent_cli/example-test.sh ./

# Mark the test script as entry point
ENTRYPOINT ["/bin/bash", "./example-test.sh"]

