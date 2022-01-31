# syntax=docker/dockerfile:1.1.3-experimental
# This Dockfile contains separate targets for each testsys agent
# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Shared build stage used to build the testsys agent binary
ARG BUILDER_IMAGE
FROM ${BUILDER_IMAGE} as build

ARG ARCH
USER root
# We need these environment variables set for building the `openssl-sys` crate
ENV PKG_CONFIG_PATH=/${ARCH}-bottlerocket-linux-musl/sys-root/usr/lib/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV CARGO_HOME=/src/.cargo
ENV OPENSSL_STATIC=true

# Build bottlerocket-agents
ADD ./ /src/
WORKDIR /src/bottlerocket-agents/
RUN --mount=type=cache,mode=0777,target=/src/target \
    cargo install --offline --locked \
      --target ${ARCH}-bottlerocket-linux-musl \
      --path . \
      --root .

# Install boringtun
RUN cargo install boringtun \
    --target ${ARCH}-bottlerocket-linux-musl \
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
COPY --from=build /src/bottlerocket-agents/bin/ec2-resource-agent ./

ENTRYPOINT ["./ec2-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the vSphere VM resource agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as vsphere-vm-resource-agent
ARG GOARCH
ARG K8S_VERSION=1.21.6

RUN yum install -y gcc tar gzip openssl-devel iproute && yum clean all

# Copy GOVC
COPY --from=build /usr/libexec/tools/govc /usr/local/bin/govc

# Install kubeadm
RUN curl -LO "https://dl.k8s.io/release/v${K8S_VERSION}/bin/linux/${GOARCH}/kubeadm" && \
    install -o root -g root -m 0755 kubeadm /usr/local/bin/kubeadm

# Copy wireguard-tools
COPY --from=wireguard-build /usr/bin/wg /usr/bin/wg
COPY --from=wireguard-build /usr/bin/wg-quick /usr/bin/wg-quick
# Copy boringtun
COPY --from=build /src/bottlerocket-agents/bin/boringtun /usr/bin/boringtun
# Copy binary
COPY --from=build /src/bottlerocket-agents/bin/vsphere-vm-resource-agent ./

ENTRYPOINT ["./vsphere-vm-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the EKS resource agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as eks-resource-agent

RUN yum install -y tar gzip && yum clean all
# Download eksctl
RUN temp_dir="$(mktemp -d --suffix eksctl-setup)" && \
    curl --silent --location "https://github.com/weaveworks/eksctl/releases/latest/download/eksctl_Linux_amd64.tar.gz" | tar xz -C /tmp && \
    mv /tmp/eksctl /usr/bin/eksctl

# Copy binary
COPY --from=build /src/bottlerocket-agents/bin/eks-resource-agent ./

ENTRYPOINT ["./eks-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the ECS resource agent image
FROM scratch as ecs-resource-agent
# Copy CA certificates store
COPY --from=build /etc/ssl /etc/ssl
COPY --from=build /etc/pki /etc/pki
# Copy binary
COPY --from=build /src/bottlerocket-agents/bin/ecs-resource-agent ./

ENTRYPOINT ["./ecs-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the ECS test agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as ecs-test-agent
# Copy binary
COPY --from=build /src/bottlerocket-agents/bin/ecs-test-agent ./

ENTRYPOINT ["./ecs-test-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the Sonobuoy test agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 AS sonobuoy-test-agent
ARG GOARCH
ARG ARCH
ARG SONOBUOY_VERSION=0.53.2
ARG SONOBUOY_URL=https://github.com/vmware-tanzu/sonobuoy/releases/download/v${SONOBUOY_VERSION}/sonobuoy_${SONOBUOY_VERSION}_linux_${GOARCH}.tar.gz
ARG AWS_IAM_AUTHENTICATOR_URL=https://amazon-eks.s3.us-west-2.amazonaws.com/1.21.2/2021-07-05/bin/linux/${GOARCH}/aws-iam-authenticator
ARG AWS_CLI_URL=https://awscli.amazonaws.com/awscli-exe-linux-${ARCH}.zip
RUN yum install -y tar gzip unzip iproute && yum clean all

# Download aws-iam-authenticator
RUN temp_dir="$(mktemp -d --suffix aws-iam-authenticator)" && \
    curl -fsSL "${AWS_IAM_AUTHENTICATOR_URL}" -o "${temp_dir}/${AWS_IAM_AUTHENTICATOR_URL##*/}" && \
    chmod 0755 "${temp_dir}/${AWS_IAM_AUTHENTICATOR_URL##*/}" && \
    mv "${temp_dir}/${AWS_IAM_AUTHENTICATOR_URL##*/}" /usr/bin/aws-iam-authenticator && \
    rm -rf ${temp_dir}

# Download aws-cli
RUN temp_dir="$(mktemp -d --suffix aws-cli)" && \
    curl -fsSL "${AWS_CLI_URL}" -o "${temp_dir}/awscliv2.zip" && \
    unzip "${temp_dir}/awscliv2.zip" -d "${temp_dir}" && \
    ${temp_dir}/aws/install && \
    rm -rf ${temp_dir}

# Download sonobuoy
RUN temp_dir="$(mktemp -d --suffix sonobuoy-setup)" && \
    curl -fsSL "${SONOBUOY_URL}" -o "${temp_dir}/${SONOBUOY_URL##*/}" && \
    tar xpf "${temp_dir}/${SONOBUOY_URL##*/}" -C "${temp_dir}" sonobuoy && \
    chmod 0755 "${temp_dir}/sonobuoy" && \
    mv "${temp_dir}/sonobuoy" /usr/bin/sonobuoy && \
    rm -rf ${temp_dir}
# Copy binary
COPY --from=build /src/bottlerocket-agents/bin/sonobuoy-test-agent ./
# Copy wireguard-tools
COPY --from=wireguard-build /usr/bin/wg /usr/bin/wg
COPY --from=wireguard-build /usr/bin/wg-quick /usr/bin/wg-quick
# Copy boringtun
COPY --from=build /src/bottlerocket-agents/bin/boringtun /usr/bin/boringtun

ENTRYPOINT ["./sonobuoy-test-agent"]

FROM public.ecr.aws/amazonlinux/amazonlinux:2 AS migration-test-agent
# Copy binary
COPY --from=build /src/bottlerocket-agents/bin/migration-test-agent ./
# Copy SSM documents
COPY --from=build /src/bottlerocket-agents/src/bin/migration-test-agent/ssm-documents/ /local/ssm-documents/

ENTRYPOINT ["./migration-test-agent"]
