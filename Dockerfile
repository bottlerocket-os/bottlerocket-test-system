# syntax=docker/dockerfile:1.1.3-experimental
# This Dockfile contains separate targets for each testsys agent
# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Shared build stage used to build the testsys agent binary
ARG BUILDER_IMAGE
ARG TOOLS_IMAGE
FROM ${BUILDER_IMAGE} as build

COPY ./ /src

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# It appears that the syntax `--from=$TOOLS_IMAGE /foo /bar` does not work. As a workaround
# we cache $TOOLS_IMAGE as a build layer.
FROM ${TOOLS_IMAGE} as tools

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build as build-go
USER builder

ARG GOARCH
ARG GOOS=linux
ARG GOROOT="/usr/libexec/go"
ARG GOPROXY

ENV PATH="${GOROOT}/bin:${PATH}"
ENV GOPROXY="${GOPROXY}"

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build as build-src
USER root
RUN mkdir -p /usr/share/licenses/testsys && \
    chown -R builder:builder /usr/share/licenses/testsys

ARG ARCH
# We need these environment variables set for building the `openssl-sys` crate
ENV PKG_CONFIG_PATH=/${ARCH}-bottlerocket-linux-musl/sys-root/usr/lib/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV CARGO_HOME=/src/.cargo
ENV OPENSSL_STATIC=true

# Build bottlerocket agents
WORKDIR /src/bottlerocket/agents/
RUN cp -p /src/LICENSE-APACHE /src/LICENSE-MIT /usr/share/licenses/testsys && \
    /usr/libexec/tools/bottlerocket-license-scan \
      --clarify /src/clarify.toml \
      --spdx-data /usr/libexec/tools/spdx-data \
      --out-dir /usr/share/licenses/testsys/vendor \
      cargo --offline --locked Cargo.toml
RUN --mount=type=cache,mode=0777,target=/src/target \
    cargo install --offline --locked \
      --target ${ARCH}-bottlerocket-linux-musl \
      --path . \
      --root .

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# TODO figure out how to build this in the Bottlerocket SDK
# Builds wireguard tools
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as wireguard-build
RUN yum install -y gcc tar gzip make && yum clean all
ARG WIREGUARD_TOOLS_VERSION=1.0.20210914
ARG WIREGUARD_TOOLS_SOURCE_URL=https://github.com/WireGuard/wireguard-tools/archive/refs/tags/v${WIREGUARD_TOOLS_VERSION}.tar.gz

# Download wireguard-tools source and install wg
RUN temp_dir="$(mktemp -d --suffix wireguard-tools-setup)" && \
    curl -fsSL "${WIREGUARD_TOOLS_SOURCE_URL}" -o "${temp_dir}/${WIREGUARD_TOOLS_SOURCE_URL##*/}" && \
    tar xpf "${temp_dir}/${WIREGUARD_TOOLS_SOURCE_URL##*/}" -C "${temp_dir}" && \
    cd "${temp_dir}/wireguard-tools-${WIREGUARD_TOOLS_VERSION}/src" && \
    make && WITH_BASHCOMPLETION=no WITH_SYSTEMDUNITS=no make install && \
    rm -rf ${temp_dir}

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the EC2 resource agent image
FROM scratch as ec2-resource-agent
# Copy CA certificates store
COPY --from=build /etc/ssl /etc/ssl
COPY --from=build /etc/pki /etc/pki
# Copy binary
COPY --from=build-src /src/bottlerocket/agents/bin/ec2-resource-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./ec2-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the vSphere VM resource agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as vsphere-vm-resource-agent

RUN yum install -y iproute && yum clean all

# Copy govc
COPY --from=build /usr/libexec/tools/govc /usr/local/bin/govc
COPY --from=build /usr/share/licenses/govmomi /licenses/govmomi

# Copy kubeadm
COPY --from=tools /kubeadm /usr/local/bin/kubeadm
COPY --from=tools /licenses/kubernetes /licenses/kubernetes

# Copy wireguard-tools binaries
COPY --from=wireguard-build /usr/bin/wg /usr/bin/wg
COPY --from=wireguard-build /usr/bin/wg-quick /usr/bin/wg-quick

# Copy boringtun binary
COPY --from=tools /boringtun /usr/bin/boringtun

# Copy binary
COPY --from=build-src /src/bottlerocket/agents/bin/vsphere-vm-resource-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./vsphere-vm-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the EKS resource agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as eks-resource-agent

# Copy eksctl
COPY --from=tools /eksctl /usr/bin/eksctl
COPY --from=tools /licenses/eksctl /licenses/eksctl

# Copy CA certificates store
COPY --from=build /etc/ssl /etc/ssl
COPY --from=build /etc/pki /etc/pki

# Copy eks-resource-agent
COPY --from=build-src /src/bottlerocket/agents/bin/eks-resource-agent ./
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./eks-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the ECS resource agent image
FROM scratch as ecs-resource-agent
# Copy CA certificates store
COPY --from=build /etc/ssl /etc/ssl
COPY --from=build /etc/pki /etc/pki
# Copy binary
COPY --from=build-src /src/bottlerocket/agents/bin/ecs-resource-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./ecs-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the ECS test agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as ecs-test-agent
# Copy binary
COPY --from=build-src /src/bottlerocket/agents/bin/ecs-test-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./ecs-test-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the Sonobuoy test agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 AS sonobuoy-test-agent
ARG ARCH

# TODO remove unzip once aws-cli moves out
RUN yum install -y unzip iproute && yum clean all
ARG AWS_CLI_URL=https://awscli.amazonaws.com/awscli-exe-linux-${ARCH}.zip

# Copy aws-iam-authenticator
COPY --from=tools /aws-iam-authenticator /usr/bin/aws-iam-authenticator
COPY --from=tools /licenses/aws-iam-authenticator /licenses/aws-iam-authenticator

# TODO move this out, get hashes, and attribute licenses
# Download aws-cli
RUN temp_dir="$(mktemp -d --suffix aws-cli)" && \
    curl -fsSL "${AWS_CLI_URL}" -o "${temp_dir}/awscliv2.zip" && \
    unzip "${temp_dir}/awscliv2.zip" -d "${temp_dir}" && \
    ${temp_dir}/aws/install && \
    rm -rf ${temp_dir}

# Copy sonobuoy
COPY --from=tools /sonobuoy /usr/bin/sonobuoy
COPY --from=tools /licenses/sonobuoy /licenses/sonobuoy

# Copy wireguard-tools
COPY --from=wireguard-build /usr/bin/wg /usr/bin/wg
COPY --from=wireguard-build /usr/bin/wg-quick /usr/bin/wg-quick

# Copy boringtun
COPY --from=tools /boringtun /usr/bin/boringtun

# Copy sonobuoy-test-agent
COPY --from=build-src /src/bottlerocket/agents/bin/sonobuoy-test-agent ./
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./sonobuoy-test-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as migration-test-agent
# Copy binary
COPY --from=build-src /src/bottlerocket/agents/bin/migration-test-agent ./
# Copy SSM documents
COPY --from=build-src /src/bottlerocket/agents/src/bin/migration-test-agent/ssm-documents/ /local/ssm-documents/
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./migration-test-agent"]
