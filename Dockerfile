# syntax=docker/dockerfile:1.1.3-experimental
# This Dockfile contains separate targets for each testsys agent
# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Shared build stage used to build the testsys agent binary
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
      --root .

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
# Builds the Sonobuoy test agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 AS sonobuoy-test-agent
ARG GOARCH
ARG ARCH
ARG SONOBUOY_VERSION=0.53.2
ARG SONOBUOY_URL=https://github.com/vmware-tanzu/sonobuoy/releases/download/v${SONOBUOY_VERSION}/sonobuoy_${SONOBUOY_VERSION}_linux_${GOARCH}.tar.gz
ARG AWS_IAM_AUTHENTICATOR_URL=https://amazon-eks.s3.us-west-2.amazonaws.com/1.21.2/2021-07-05/bin/linux/${GOARCH}/aws-iam-authenticator
ARG AWS_CLI_URL=https://awscli.amazonaws.com/awscli-exe-linux-${ARCH}.zip
RUN yum install -y tar gzip unzip && yum clean all

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

ENTRYPOINT ["./sonobuoy-test-agent"]

FROM public.ecr.aws/amazonlinux/amazonlinux:2 AS migration-test-agent
# Copy binary
COPY --from=build /src/bottlerocket-agents/bin/migration-test-agent ./
# Copy SSM documents
COPY --from=build /src/bottlerocket-agents/src/bin/migration-test-agent/ssm-documents/ /local/ssm-documents/

ENTRYPOINT ["./migration-test-agent"]
