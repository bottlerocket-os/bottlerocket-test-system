ARG ARCH
FROM bottlerocket-sdk-openssl-${ARCH} as build
ARG ARCH
USER root

ADD ./ /src/
WORKDIR /src/agent/test-agent
RUN cargo install --locked --target ${ARCH}-bottlerocket-linux-musl --path . --example example_test_agent --root ./

FROM alpine

COPY --from=build /src/agent/test-agent/bin/example_test_agent ./

ENTRYPOINT ["./example_test_agent"]
