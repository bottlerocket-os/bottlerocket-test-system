# syntax=docker/dockerfile:1.1.3-experimental
ARG BUILDER_IMAGE
FROM ${BUILDER_IMAGE} as build

ARG ARCH
USER root

ENV CARGO_HOME=/src/.cargo

ADD ./ /src/
WORKDIR /src/agent/test-agent
RUN --mount=type=cache,mode=0777,target=/src/target \
    cargo install --offline --locked --target ${ARCH}-bottlerocket-linux-musl --path . --example example_test_agent --root ./

FROM public.ecr.aws/amazonlinux/amazonlinux:2

COPY --from=build /src/agent/test-agent/bin/example_test_agent ./

ENTRYPOINT ["./example_test_agent"]
