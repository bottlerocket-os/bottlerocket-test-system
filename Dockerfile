# syntax=docker/dockerfile:1.1.3-experimental
# This Dockfile contains separate targets for each testsys agent
# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Shared build stage used to build the testsys agent binary
ARG BUILDER_IMAGE
FROM ${BUILDER_IMAGE} as build

ADD ./ /src

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build as build-go
USER builder

ARG GOARCH
ARG GOROOT="/usr/libexec/go"

ENV PATH="${GOROOT}/bin:${PATH}"

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build as build-src
USER root

ARG ARCH
# We need these environment variables set for building the `openssl-sys` crate
ENV PKG_CONFIG_PATH=/${ARCH}-bottlerocket-linux-musl/sys-root/usr/lib/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV CARGO_HOME=/src/.cargo
ENV OPENSSL_STATIC=true

# Build bottlerocket-agents
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
FROM build as eksctl-build

USER root

ARG EKSCTL_VERSION=0.82.0
ARG GOARCH
ARG EKSCTL_BINARY_URL="https://github.com/weaveworks/eksctl/releases/download/v${EKSCTL_VERSION}/eksctl_Linux_${GOARCH}.tar.gz"

USER builder
WORKDIR /home/builder/
RUN curl -OL "${EKSCTL_BINARY_URL}" && \
    tar -xf eksctl_Linux_${GOARCH}.tar.gz -C /tmp && \
    rm eksctl_Linux_${GOARCH}.tar.gz

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build as kubernetes-build

USER root

ARG K8S_VERSION=1.21.6
ARG GOARCH
ARG KUBEADM_BINARY_URL="https://dl.k8s.io/release/v${K8S_VERSION}/bin/linux/${GOARCH}/kubeadm"

USER builder
WORKDIR /home/builder/
RUN curl -L ${KUBEADM_BINARY_URL} -o kubeadm.${GOARCH} && \
    install -m 0755 kubeadm.${GOARCH} /tmp/kubeadm

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build as sonobuoy-build

USER root

ARG SONOBUOY_VERSION=0.53.2
ARG GOARCH
ARG SONOBUOY_BINARY_URL="https://github.com/vmware-tanzu/sonobuoy/releases/download/v${SONOBUOY_VERSION}/sonobuoy_${SONOBUOY_VERSION}_linux_${GOARCH}.tar.gz"

USER builder
WORKDIR /home/builder/
RUN curl -OL ${SONOBUOY_BINARY_URL} && \
    tar -xf sonobuoy_${SONOBUOY_VERSION}_linux_${GOARCH}.tar.gz -C /tmp && \
    chmod 0755 /tmp/sonobuoy && \
    rm sonobuoy_${SONOBUOY_VERSION}_linux_${GOARCH}.tar.gz

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build-go as aws-iam-authenticator-build

USER root
RUN mkdir -p /usr/share/licenses/aws-iam-authenticator && \
    chown -R builder:builder /usr/share/licenses/aws-iam-authenticator

ARG AWS_IAM_AUTHENTICATOR_VERSION=0.5.3
ARG AWS_IAM_AUTHENTICATOR_SOURCE_URL="https://github.com/kubernetes-sigs/aws-iam-authenticator/archive/refs/tags/v${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz"

USER builder
WORKDIR /home/builder/
RUN mkdir aws-iam-authenticator && curl -L ${AWS_IAM_AUTHENTICATOR_SOURCE_URL} \
      -o aws-iam-authenticator-${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz && \
    tar -xf aws-iam-authenticator-${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz --strip-components 1 \
      -C aws-iam-authenticator && \
    rm aws-iam-authenticator-${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz

WORKDIR /home/builder/aws-iam-authenticator/
RUN go mod vendor
RUN go build -mod=vendor -o /tmp/aws-iam-authenticator \
      ./cmd/aws-iam-authenticator

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the EC2 resource agent image
FROM scratch as ec2-resource-agent
# Copy CA certificates store
COPY --from=build /etc/ssl /etc/ssl
COPY --from=build /etc/pki /etc/pki
# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/ec2-resource-agent ./

ENTRYPOINT ["./ec2-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the vSphere VM resource agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as vsphere-vm-resource-agent

RUN yum install -y iproute && yum clean all

# Copy govc binary
COPY --from=build /usr/libexec/tools/govc /usr/local/bin/govc

# Copy kubeadm binary
COPY --from=kubernetes-build /tmp/kubeadm /usr/local/bin/kubeadm

# Copy wireguard-tools binaries
COPY --from=wireguard-build /usr/bin/wg /usr/bin/wg
COPY --from=wireguard-build /usr/bin/wg-quick /usr/bin/wg-quick

# Copy boringtun binary
COPY --from=build-src /src/bottlerocket-agents/bin/boringtun /usr/bin/boringtun

# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/vsphere-vm-resource-agent ./

ENTRYPOINT ["./vsphere-vm-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the EKS resource agent image
FROM scratch as eks-resource-agent

# Copy eksctl binary
COPY --from=eksctl-build /tmp/eksctl /usr/bin/eksctl

# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/eks-resource-agent ./

ENTRYPOINT ["./eks-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the ECS resource agent image
FROM scratch as ecs-resource-agent
# Copy CA certificates store
COPY --from=build /etc/ssl /etc/ssl
COPY --from=build /etc/pki /etc/pki
# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/ecs-resource-agent ./

ENTRYPOINT ["./ecs-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the ECS test agent image
FROM scratch as ecs-test-agent
# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/ecs-test-agent ./

ENTRYPOINT ["./ecs-test-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the Sonobuoy test agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 AS sonobuoy-test-agent
ARG ARCH
RUN yum install -y unzip iproute && yum clean all
ARG AWS_CLI_URL=https://awscli.amazonaws.com/awscli-exe-linux-${ARCH}.zip

# Copy aws-iam-authenticator binary
COPY --from=aws-iam-authenticator-build /tmp/aws-iam-authenticator /usr/bin/aws-iam-authenticator

# Download aws-cli
RUN temp_dir="$(mktemp -d --suffix aws-cli)" && \
    curl -fsSL "${AWS_CLI_URL}" -o "${temp_dir}/awscliv2.zip" && \
    unzip "${temp_dir}/awscliv2.zip" -d "${temp_dir}" && \
    ${temp_dir}/aws/install && \
    rm -rf ${temp_dir}

# Copy sonobuoy binary
COPY --from=sonobuoy-build /tmp/sonobuoy /usr/bin/sonobuoy

# Copy wireguard-tools
COPY --from=wireguard-build /usr/bin/wg /usr/bin/wg
COPY --from=wireguard-build /usr/bin/wg-quick /usr/bin/wg-quick

# Copy boringtun
COPY --from=build-src /src/bottlerocket-agents/bin/boringtun /usr/bin/boringtun

# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/sonobuoy-test-agent ./

ENTRYPOINT ["./sonobuoy-test-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM scratch as migration-test-agent
# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/migration-test-agent ./
# Copy SSM documents
COPY --from=build-src /src/bottlerocket-agents/src/bin/migration-test-agent/ssm-documents/ /local/ssm-documents/

ENTRYPOINT ["./migration-test-agent"]
