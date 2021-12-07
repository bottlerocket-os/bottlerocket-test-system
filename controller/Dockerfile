# syntax=docker/dockerfile:1.1.3-experimental
ARG ARCH
FROM public.ecr.aws/bottlerocket/bottlerocket-sdk-${ARCH}:v0.23.0 as build

ARG ARCH
USER root
# We need these environment variables set for building the `openssl-sys` crate
ENV PKG_CONFIG_PATH=/${ARCH}-bottlerocket-linux-musl/sys-root/usr/lib/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV CARGO_HOME=/src/.cargo
ENV OPENSSL_STATIC=true
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
