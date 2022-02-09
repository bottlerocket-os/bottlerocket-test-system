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
ARG GOOS=linux
ARG GOROOT="/usr/libexec/go"

ENV PATH="${GOROOT}/bin:${PATH}"

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

# Build bottlerocket-agents
WORKDIR /src/bottlerocket-agents/
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

# TODO get licenses for boringtun
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
FROM build-go as eksctl-build

USER root
RUN mkdir -p /usr/share/licenses/eksctl && \
    chown -R builder:builder /usr/share/licenses/eksctl

ARG EKSCTL_VERSION=0.82.0
ARG EKSCTL_SOURCE_URL="https://github.com/weaveworks/eksctl/archive/refs/tags/v${EKSCTL_VERSION}.tar.gz"

ARG GOARCH
ARG EKSCTL_BINARY_URL="https://github.com/weaveworks/eksctl/releases/download/v${EKSCTL_VERSION}/eksctl_Linux_${GOARCH}.tar.gz"

USER builder
WORKDIR /home/builder/
RUN mkdir eksctl && curl -L ${EKSCTL_SOURCE_URL} \
      -o eksctl_${EKSCTL_VERSION}.tar.gz && \
    grep eksctl_${EKSCTL_VERSION}.tar.gz \
      /src/hashes/eksctl | sha512sum --check - && \
    tar -xf eksctl_${EKSCTL_VERSION}.tar.gz --strip-components 1 -C eksctl && \
    rm eksctl_${EKSCTL_VERSION}.tar.gz

WORKDIR /home/builder/eksctl/
RUN go mod vendor
RUN cp -p LICENSE /usr/share/licenses/eksctl && \
    /usr/libexec/tools/bottlerocket-license-scan \
      --clarify /src/clarify.toml \
      --spdx-data /usr/libexec/tools/spdx-data \
      --out-dir /usr/share/licenses/eksctl/vendor \
      go-vendor ./vendor
RUN curl -L "${EKSCTL_BINARY_URL}" \
      -o eksctl_${EKSCTL_VERSION}_${GOOS}_${GOARCH}.tar.gz && \
    grep eksctl_${EKSCTL_VERSION}_${GOOS}_${GOARCH}.tar.gz \
      /src/hashes/eksctl | sha512sum --check - && \
    tar -xf eksctl_${EKSCTL_VERSION}_${GOOS}_${GOARCH}.tar.gz -C /tmp && \
    rm eksctl_${EKSCTL_VERSION}_${GOOS}_${GOARCH}.tar.gz

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build-go as kubernetes-build

USER root
RUN mkdir -p /usr/share/licenses/kubernetes && \
    chown -R builder:builder /usr/share/licenses/kubernetes

ARG K8S_VERSION=1.21.6
ARG K8S_SOURCE_URL="https://github.com/kubernetes/kubernetes/archive/refs/tags/v${K8S_VERSION}.tar.gz"

ARG GOARCH
ARG KUBEADM_BINARY_URL="https://dl.k8s.io/release/v${K8S_VERSION}/bin/linux/${GOARCH}/kubeadm"

USER builder
WORKDIR /home/builder/
RUN mkdir kubernetes && \
    curl -L "${K8S_SOURCE_URL}" -o kubernetes_${K8S_VERSION}.tar.gz && \
    grep kubernetes_${K8S_VERSION}.tar.gz \
      /src/hashes/kubernetes | sha512sum --check - && \
    tar -xf kubernetes_${K8S_VERSION}.tar.gz \
      --strip-components 1 -C kubernetes && \
    rm kubernetes_${K8S_VERSION}.tar.gz

WORKDIR /home/builder/kubernetes/
RUN go mod vendor
RUN cp -p LICENSE /usr/share/licenses/kubernetes && \
    /usr/libexec/tools/bottlerocket-license-scan \
      --clarify /src/clarify.toml \
      --spdx-data /usr/libexec/tools/spdx-data \
      --out-dir /usr/share/licenses/kubernetes/vendor \
      go-vendor ./vendor
RUN curl -L ${KUBEADM_BINARY_URL} \
      -o kubeadm_${K8S_VERSION}_${GOOS}_${GOARCH} && \
    grep kubeadm_${K8S_VERSION}_${GOOS}_${GOARCH} \
      /src/hashes/kubernetes | sha512sum --check - && \
    install -m 0755 kubeadm_${K8S_VERSION}_${GOOS}_${GOARCH} /tmp/kubeadm

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build-go as sonobuoy-build

USER root
RUN mkdir -p /usr/share/licenses/sonobuoy && \
    chown -R builder:builder /usr/share/licenses/sonobuoy

ARG SONOBUOY_VERSION=0.53.2
ARG SONOBUOY_SOURCE_URL="https://github.com/vmware-tanzu/sonobuoy/archive/refs/tags/v${SONOBUOY_VERSION}.tar.gz"

ARG GOARCH
ARG SONOBUOY_BINARY_URL="https://github.com/vmware-tanzu/sonobuoy/releases/download/v${SONOBUOY_VERSION}/sonobuoy_${SONOBUOY_VERSION}_linux_${GOARCH}.tar.gz"

USER builder
WORKDIR /home/builder/
RUN mkdir sonobuoy && \
    curl -L "${SONOBUOY_SOURCE_URL}" -o sonobuoy_${SONOBUOY_VERSION}.tar.gz && \
    grep sonobuoy_${SONOBUOY_VERSION}.tar.gz \
      /src/hashes/sonobuoy | sha512sum --check - && \
    tar -xf sonobuoy_${SONOBUOY_VERSION}.tar.gz \
      --strip-components 1 -C sonobuoy && \
    rm sonobuoy_${SONOBUOY_VERSION}.tar.gz

WORKDIR /home/builder/sonobuoy/
RUN go mod vendor
RUN cp -p LICENSE /usr/share/licenses/sonobuoy && \
    /usr/libexec/tools/bottlerocket-license-scan \
      --clarify /src/clarify.toml \
      --spdx-data /usr/libexec/tools/spdx-data \
      --out-dir /usr/share/licenses/sonobuoy/vendor \
      go-vendor ./vendor
RUN curl -OL ${SONOBUOY_BINARY_URL} && \
    grep sonobuoy_${SONOBUOY_VERSION}_${GOOS}_${GOARCH}.tar.gz \
      /src/hashes/sonobuoy | sha512sum --check - && \
    tar -xf sonobuoy_${SONOBUOY_VERSION}_${GOOS}_${GOARCH}.tar.gz -C /tmp && \
    chmod 0755 /tmp/sonobuoy && \
    rm sonobuoy_${SONOBUOY_VERSION}_${GOOS}_${GOARCH}.tar.gz

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build-go as aws-iam-authenticator-build

USER root
RUN mkdir -p /usr/share/licenses/aws-iam-authenticator && \
    chown -R builder:builder /usr/share/licenses/aws-iam-authenticator

ARG AWS_IAM_AUTHENTICATOR_VERSION=0.5.3
ARG AWS_IAM_AUTHENTICATOR_SOURCE_URL="https://github.com/kubernetes-sigs/aws-iam-authenticator/archive/refs/tags/v${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz"

USER builder
WORKDIR /home/builder/
RUN mkdir aws-iam-authenticator && \
    curl -L ${AWS_IAM_AUTHENTICATOR_SOURCE_URL} \
      -o aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz && \
    grep aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz \
      /src/hashes/aws-iam-authenticator | sha512sum --check - && \
    tar -xf aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz \
      --strip-components 1 -C aws-iam-authenticator && \
    rm aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz

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
# Copy govc licenses
COPY --from=build /usr/share/licenses/govmomi /licenses/govmomi

# Copy kubeadm binary
COPY --from=kubernetes-build /tmp/kubeadm /usr/local/bin/kubeadm
# Copy kubeadm licenses
COPY --from=kubernetes-build /usr/share/licenses/kubernetes /licenses/kubernetes

# Copy wireguard-tools binaries
COPY --from=wireguard-build /usr/bin/wg /usr/bin/wg
COPY --from=wireguard-build /usr/bin/wg-quick /usr/bin/wg-quick

# Copy boringtun binary
COPY --from=build-src /src/bottlerocket-agents/bin/boringtun /usr/bin/boringtun

# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/vsphere-vm-resource-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./vsphere-vm-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the EKS resource agent image
FROM scratch as eks-resource-agent

# Copy eksctl binary
COPY --from=eksctl-build /tmp/eksctl /usr/bin/eksctl
# Copy eksctl licenses
COPY --from=eksctl-build /usr/share/licenses/eksctl /licenses/eksctl

# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/eks-resource-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./eks-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the ECS resource agent image
FROM scratch as ecs-resource-agent
# Copy CA certificates store
COPY --from=build /etc/ssl /etc/ssl
COPY --from=build /etc/pki /etc/pki
# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/ecs-resource-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./ecs-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the ECS test agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as ecs-test-agent
# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/ecs-test-agent ./
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

# Copy aws-iam-authenticator binary
COPY --from=aws-iam-authenticator-build /tmp/aws-iam-authenticator /usr/bin/aws-iam-authenticator
# Copy aws-iam-authenticator licenses
COPY --from=aws-iam-authenticator-build /usr/share/licenses/aws-iam-authenticator /licenses/aws-iam-authenticator

# TODO move this out, get hashes, and attribute licenses
# Download aws-cli
RUN temp_dir="$(mktemp -d --suffix aws-cli)" && \
    curl -fsSL "${AWS_CLI_URL}" -o "${temp_dir}/awscliv2.zip" && \
    unzip "${temp_dir}/awscliv2.zip" -d "${temp_dir}" && \
    ${temp_dir}/aws/install && \
    rm -rf ${temp_dir}

# Copy sonobuoy binary
COPY --from=sonobuoy-build /tmp/sonobuoy /usr/bin/sonobuoy
# Copy sonobuoy licenses
COPY --from=sonobuoy-build /usr/share/licenses/sonobuoy /licenses/sonobuoy

# Copy wireguard-tools
COPY --from=wireguard-build /usr/bin/wg /usr/bin/wg
COPY --from=wireguard-build /usr/bin/wg-quick /usr/bin/wg-quick

# Copy boringtun
COPY --from=build-src /src/bottlerocket-agents/bin/boringtun /usr/bin/boringtun

# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/sonobuoy-test-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./sonobuoy-test-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as migration-test-agent
# Copy binary
COPY --from=build-src /src/bottlerocket-agents/bin/migration-test-agent ./
# Copy SSM documents
COPY --from=build-src /src/bottlerocket-agents/src/bin/migration-test-agent/ssm-documents/ /local/ssm-documents/
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./migration-test-agent"]
