# syntax=docker/dockerfile:1.1.3-experimental
# This Dockfile contains separate targets for each testsys agent
# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Shared build stage used to build Go binaries.
ARG BUILDER_IMAGE
FROM ${BUILDER_IMAGE} as build-go
USER builder

ARG GOARCH
ARG GOOS=linux
ARG GOROOT="/usr/libexec/go"
ARG GOPROXY

ENV PATH="${GOROOT}/bin:${PATH}"
ENV GOPROXY="${GOPROXY}"

ADD ./hashes /hashes
COPY ./clarify.toml /clarify.toml

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Shared build stage used to build Rust binaries.
ARG BUILDER_IMAGE
FROM ${BUILDER_IMAGE} as build-rust
ARG ARCH

USER root
RUN mkdir -p /home/builder && \
    chown -R builder:builder /home/builder
USER builder
WORKDIR /home/builder

# We need these environment variables set for building the `openssl-sys` crate
ENV PKG_CONFIG_PATH=/${ARCH}-bottlerocket-linux-musl/sys-root/usr/lib/pkgconfig
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV CARGO_HOME=/home/builder/.cargo
ENV OPENSSL_STATIC=true

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build-go as eksctl-build

USER root
RUN mkdir -p /usr/share/licenses/eksctl && \
    chown -R builder:builder /usr/share/licenses/eksctl && \
    mkdir -p /usr/share/licenses/eksctl-anywhere && \
    chown -R builder:builder /usr/share/licenses/eksctl-anywhere

ARG EKSCTL_VERSION=0.112.0
ARG EKSCTL_SOURCE_URL="https://github.com/weaveworks/eksctl/archive/refs/tags/v${EKSCTL_VERSION}.tar.gz"

ARG GOARCH
ARG EKSCTL_BINARY_URL="https://github.com/weaveworks/eksctl/releases/download/v${EKSCTL_VERSION}/eksctl_Linux_${GOARCH}.tar.gz"

ARG EKSA_VERSION=0.12.1
ARG EKSA_SOURCE_URL="https://github.com/aws/eks-anywhere/archive/refs/tags/v${EKSA_VERSION}.tar.gz"
ARG EKSA_RELEASE_NUMBER=23
# TODO EKS-A currently does not provide an ARM64 binary, add the hash for it when they do
ARG EKSA_BINARY_URL="https://anywhere-assets.eks.amazonaws.com/releases/eks-a/${EKSA_RELEASE_NUMBER}/artifacts/eks-a/v${EKSA_VERSION}/${GOOS}/amd64/eksctl-anywhere-v${EKSA_VERSION}-${GOOS}-amd64.tar.gz"

USER builder
WORKDIR /home/builder/
# eksctl
RUN mkdir eksctl && curl -L ${EKSCTL_SOURCE_URL} \
      -o eksctl_${EKSCTL_VERSION}.tar.gz && \
    grep eksctl_${EKSCTL_VERSION}.tar.gz \
      /hashes/eksctl | sha512sum --check - && \
    tar -xf eksctl_${EKSCTL_VERSION}.tar.gz --strip-components 1 -C eksctl && \
    rm eksctl_${EKSCTL_VERSION}.tar.gz

WORKDIR /home/builder/eksctl/
# TODO - remove this workaround for a bad go.sum hash
# caused by https://github.com/blizzy78/varnamelen/issues/13#issuecomment-1079040934
# these sed statement replace a hash that has changed. -i.bak makes these compatible with BSD/macOS sed.
RUN \
  sed -i 's,github.com/blizzy78/varnamelen v0.6.1 h1:kttPCLzXFa+0nt++Cw9fb7GrSSM4KkyIAoX/vXsbuqA=,github.com/blizzy78/varnamelen v0.6.1 h1:iYAU/3A6cpfRm2ZI0P/lece4jsc7GEbzsxTu+vBCChQ=,' go.sum ; \
  sed -i 's,github.com/blizzy78/varnamelen v0.6.1/go.mod h1:zy2Eic4qWqjrxa60jG34cfL0VXcSwzUrIx68eJPb4Q8=,github.com/blizzy78/varnamelen v0.6.1/go.mod h1:mGBHm+Uo4e8JnZEKHRoZgVEOQdSBdQfY/x+k4NAXBWA=,' go.sum
RUN go mod vendor
RUN cp -p LICENSE /usr/share/licenses/eksctl && \
    /usr/libexec/tools/bottlerocket-license-scan \
      --clarify /clarify.toml \
      --spdx-data /usr/libexec/tools/spdx-data \
      --out-dir /usr/share/licenses/eksctl/vendor \
      go-vendor ./vendor
RUN curl -L "${EKSCTL_BINARY_URL}" \
      -o eksctl_${EKSCTL_VERSION}_${GOOS}_${GOARCH}.tar.gz && \
    grep eksctl_${EKSCTL_VERSION}_${GOOS}_${GOARCH}.tar.gz \
      /hashes/eksctl | sha512sum --check - && \
    tar -xf eksctl_${EKSCTL_VERSION}_${GOOS}_${GOARCH}.tar.gz -C /tmp && \
    rm eksctl_${EKSCTL_VERSION}_${GOOS}_${GOARCH}.tar.gz

# eksctl-anywhere
# TODO Try and build this; it's not straightforward with the tight coupling between EKS-A releases and its bundles
WORKDIR /home/builder/
RUN mkdir eksctl-anywhere && curl -L ${EKSA_SOURCE_URL} \
      -o eksctl_anywhere_${EKSA_VERSION}.tar.gz && \
    grep eksctl_anywhere_${EKSA_VERSION}.tar.gz \
      /hashes/eksctl | sha512sum --check - && \
    tar -xf eksctl_anywhere_${EKSA_VERSION}.tar.gz --strip-components 1 -C eksctl-anywhere && \
    rm eksctl_anywhere_${EKSA_VERSION}.tar.gz && \
    sed -i 's/v0.2.0-alpha.6/v0.2.1/' eksctl-anywhere/go.mod && \
    rm eksctl-anywhere/go.sum

USER root
WORKDIR /home/builder/eksctl-anywhere/
RUN go mod tidy && go mod vendor
RUN cp -p LICENSE /usr/share/licenses/eksctl-anywhere && \
    /usr/libexec/tools/bottlerocket-license-scan \
      --clarify /clarify.toml \
      --spdx-data /usr/libexec/tools/spdx-data \
      --out-dir /usr/share/licenses/eksctl-anywhere/vendor \
      go-vendor ./vendor
RUN curl -L "${EKSA_BINARY_URL}" \
      -o eksctl_anywhere_${EKSA_VERSION}_${EKSA_RELEASE_NUMBER}_${GOOS}_amd64.tar.gz && \
    grep eksctl_anywhere_${EKSA_VERSION}_${EKSA_RELEASE_NUMBER}_${GOOS}_amd64.tar.gz \
      /hashes/eksctl | sha512sum --check - && \
    tar -xzf eksctl_anywhere_${EKSA_VERSION}_${EKSA_RELEASE_NUMBER}_${GOOS}_amd64.tar.gz -C /tmp && \
    rm eksctl_anywhere_${EKSA_VERSION}_${EKSA_RELEASE_NUMBER}_${GOOS}_amd64.tar.gz

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build-go as kubernetes-build

USER root
RUN mkdir -p /usr/share/licenses/kubernetes && \
    chown -R builder:builder /usr/share/licenses/kubernetes

ARG K8S_VERSION=1.23.13
ARG K8S_SOURCE_URL="https://github.com/kubernetes/kubernetes/archive/refs/tags/v${K8S_VERSION}.tar.gz"

ARG GOARCH
ARG KUBEADM_BINARY_URL="https://dl.k8s.io/release/v${K8S_VERSION}/bin/linux/${GOARCH}/kubeadm"
ARG KUBECTL_BINARY_URL="https://dl.k8s.io/release/v${K8S_VERSION}/bin/linux/${GOARCH}/kubectl"

USER builder
WORKDIR /home/builder/
RUN mkdir kubernetes && \
    curl -L "${K8S_SOURCE_URL}" -o kubernetes_${K8S_VERSION}.tar.gz && \
    grep kubernetes_${K8S_VERSION}.tar.gz \
      /hashes/kubernetes | sha512sum --check - && \
    tar -xf kubernetes_${K8S_VERSION}.tar.gz \
      --strip-components 1 -C kubernetes && \
    rm kubernetes_${K8S_VERSION}.tar.gz

WORKDIR /home/builder/kubernetes/
RUN go mod vendor
RUN cp -p LICENSE /usr/share/licenses/kubernetes && \
    /usr/libexec/tools/bottlerocket-license-scan \
      --clarify /clarify.toml \
      --spdx-data /usr/libexec/tools/spdx-data \
      --out-dir /usr/share/licenses/kubernetes/vendor \
      go-vendor ./vendor
RUN curl -L ${KUBEADM_BINARY_URL} \
      -o kubeadm_${K8S_VERSION}_${GOOS}_${GOARCH} && \
    grep kubeadm_${K8S_VERSION}_${GOOS}_${GOARCH} \
      /hashes/kubernetes | sha512sum --check - && \
    install -m 0755 kubeadm_${K8S_VERSION}_${GOOS}_${GOARCH} /tmp/kubeadm
RUN curl -L ${KUBECTL_BINARY_URL} \
      -o kubectl_${K8S_VERSION}_${GOOS}_${GOARCH} && \
    grep kubectl_${K8S_VERSION}_${GOOS}_${GOARCH} \
      /hashes/kubernetes | sha512sum --check - && \
    install -m 0755 kubectl_${K8S_VERSION}_${GOOS}_${GOARCH} /tmp/kubectl

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build-go as sonobuoy-build

USER root
RUN mkdir -p /usr/share/licenses/sonobuoy && \
    chown -R builder:builder /usr/share/licenses/sonobuoy

ARG SONOBUOY_VERSION=0.56.4
ARG SONOBUOY_SOURCE_URL="https://github.com/vmware-tanzu/sonobuoy/archive/refs/tags/v${SONOBUOY_VERSION}.tar.gz"

ARG GOARCH
ARG SONOBUOY_BINARY_URL="https://github.com/vmware-tanzu/sonobuoy/releases/download/v${SONOBUOY_VERSION}/sonobuoy_${SONOBUOY_VERSION}_linux_${GOARCH}.tar.gz"

USER builder
WORKDIR /home/builder/
RUN mkdir sonobuoy && \
    curl -L "${SONOBUOY_SOURCE_URL}" -o sonobuoy_${SONOBUOY_VERSION}.tar.gz && \
    grep sonobuoy_${SONOBUOY_VERSION}.tar.gz \
      /hashes/sonobuoy | sha512sum --check - && \
    tar -xf sonobuoy_${SONOBUOY_VERSION}.tar.gz \
      --strip-components 1 -C sonobuoy && \
    rm sonobuoy_${SONOBUOY_VERSION}.tar.gz

WORKDIR /home/builder/sonobuoy/
RUN go mod vendor
RUN cp -p LICENSE /usr/share/licenses/sonobuoy && \
    /usr/libexec/tools/bottlerocket-license-scan \
      --clarify /clarify.toml \
      --spdx-data /usr/libexec/tools/spdx-data \
      --out-dir /usr/share/licenses/sonobuoy/vendor \
      go-vendor ./vendor
RUN curl -OL ${SONOBUOY_BINARY_URL} && \
    grep sonobuoy_${SONOBUOY_VERSION}_${GOOS}_${GOARCH}.tar.gz \
      /hashes/sonobuoy | sha512sum --check - && \
    tar -xf sonobuoy_${SONOBUOY_VERSION}_${GOOS}_${GOARCH}.tar.gz -C /tmp && \
    chmod 0755 /tmp/sonobuoy && \
    rm sonobuoy_${SONOBUOY_VERSION}_${GOOS}_${GOARCH}.tar.gz

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
FROM build-go as aws-iam-authenticator-build

USER root
RUN mkdir -p /usr/share/licenses/aws-iam-authenticator && \
    chown -R builder:builder /usr/share/licenses/aws-iam-authenticator

ARG AWS_IAM_AUTHENTICATOR_VERSION=0.5.3
ARG AWS_IAM_AUTHENTICATOR_SHA512_SUM=430af9fd04b9a94205a485281fb668f5bc18cdac569de0232fa98e08ebb0e08a8d233537bd3373a5f1e53cf529bc2050aebc34a4a53c8b29a831070e34213210
ARG AWS_IAM_AUTHENTICATOR_SOURCE_URL="https://cache.bottlerocket.aws/aws-iam-authenticator-${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz/${AWS_IAM_AUTHENTICATOR_SHA512_SUM}/aws-iam-authenticator-${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz"

USER builder
WORKDIR /home/builder/
RUN mkdir aws-iam-authenticator && \
    curl -L ${AWS_IAM_AUTHENTICATOR_SOURCE_URL} \
      -o aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz && \
    grep aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz \
      /hashes/aws-iam-authenticator | sha512sum --check - && \
    tar -xf aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz \
      --strip-components 1 -C aws-iam-authenticator && \
    rm aws-iam-authenticator_${AWS_IAM_AUTHENTICATOR_VERSION}.tar.gz

WORKDIR /home/builder/aws-iam-authenticator/
RUN go mod vendor
RUN CGO_ENABLED=0 go build -mod=vendor -o /tmp/aws-iam-authenticator \
      ./cmd/aws-iam-authenticator

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Package the binaries for use by other container image builds.
FROM scratch
# aws-iam-authenticator
COPY --from=aws-iam-authenticator-build /tmp/aws-iam-authenticator /aws-iam-authenticator
COPY --from=aws-iam-authenticator-build /usr/share/licenses/aws-iam-authenticator /licenses/aws-iam-authenticator

# eksctl
COPY --from=eksctl-build /tmp/eksctl /eksctl
COPY --from=eksctl-build /usr/share/licenses/eksctl /licenses/eksctl

# eksctl-anywhere
COPY --from=eksctl-build /tmp/eksctl-anywhere /eksctl-anywhere
COPY --from=eksctl-build /usr/share/licenses/eksctl-anywhere /licenses/eksctl-anywhere

# kubeadm and kubectl
COPY --from=kubernetes-build /tmp/kubeadm /kubeadm
COPY --from=kubernetes-build /tmp/kubectl /kubectl
COPY --from=kubernetes-build /usr/share/licenses/kubernetes /licenses/kubernetes

# sonobuoy
COPY --from=sonobuoy-build /tmp/sonobuoy /sonobuoy
COPY --from=sonobuoy-build /usr/share/licenses/sonobuoy /licenses/sonobuoy
