ARG ARCH
FROM bottlerocket-sdk-openssl-${ARCH} as build
ARG ARCH
USER root

ADD ./ /src/
WORKDIR /src/controller
RUN cargo install --locked --target ${ARCH}-bottlerocket-linux-musl --path . --root ./

FROM scratch
# Copy CA certificates store
COPY --from=public.ecr.aws/amazonlinux/amazonlinux:2 /etc/ssl /etc/ssl
COPY --from=public.ecr.aws/amazonlinux/amazonlinux:2 /etc/pki /etc/pki
COPY --from=build /src/controller/bin/controller ./

ENTRYPOINT ["./controller"]
