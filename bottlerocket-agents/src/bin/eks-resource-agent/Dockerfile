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
WORKDIR /src/bottlerocket-agents/
RUN --mount=type=cache,mode=0777,target=/src/target \
    cargo install --offline --locked \
      --target ${ARCH}-bottlerocket-linux-musl \
      --path . \
      --root . \
      --bin eks-resource-agent

FROM public.ecr.aws/amazonlinux/amazonlinux:2

RUN yum install -y tar gzip && yum clean all
# Download eksctl
RUN temp_dir="$(mktemp -d --suffix eksctl-setup)" && \
    curl --silent --location "https://github.com/weaveworks/eksctl/releases/latest/download/eksctl_Linux_amd64.tar.gz" | tar xz -C /tmp && \
    mv /tmp/eksctl /usr/bin/eksctl

# Copy binary
COPY --from=build /src/bottlerocket-agents/bin/eks-resource-agent ./

ENTRYPOINT ["./eks-resource-agent"]
