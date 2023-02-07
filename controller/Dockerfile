# syntax=docker/dockerfile:1.1.3-experimental
ARG BUILDER_IMAGE
FROM ${BUILDER_IMAGE} as build

ARG ARCH
USER root

ENV CARGO_HOME=/src/.cargo

ADD ./ /src/
WORKDIR /src/controller
RUN --mount=type=cache,mode=0777,target=/src/target \
    cargo install --offline --locked --target ${ARCH}-bottlerocket-linux-musl --path . --root ./

FROM scratch
# Copy CA certificates store
COPY --from=build /etc/ssl /etc/ssl
COPY --from=build /etc/pki /etc/pki
# Copy binary
COPY --from=build /src/controller/bin/controller ./

ENTRYPOINT ["./controller"]
