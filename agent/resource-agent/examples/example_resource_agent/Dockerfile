ARG ARCH
FROM bottlerocket-sdk-openssl-${ARCH} as build
ARG ARCH
USER root

ADD ./ /src/
WORKDIR /src/agent/resource-agent
RUN cargo install --locked --target ${ARCH}-bottlerocket-linux-musl --path . --example example_resource_agent --root ./

FROM scratch
# Copy CA certificates store
COPY --from=build /etc/ssl /etc/ssl
COPY --from=build /etc/pki /etc/pki
# Copy binary
COPY --from=build /src/agent/resource-agent/bin/example_resource_agent ./

ENTRYPOINT ["./example_resource_agent"]
