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

ENV CARGO_HOME=/src/.cargo

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
# Builds the EC2 karpenter resource agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as ec2-karpenter-resource-agent

RUN yum install -y tar && yum -y clean all && rm -fr /var/cache

# Copy eksctl
COPY --from=tools /eksctl /usr/bin/eksctl
COPY --from=tools /licenses/eksctl /licenses/eksctl

# Copy CA certificates store
COPY --from=build /etc/ssl /etc/ssl
COPY --from=build /etc/pki /etc/pki

# Copy aws-iam-authenticator
COPY --from=tools /aws-iam-authenticator /usr/bin/aws-iam-authenticator
COPY --from=tools /licenses/aws-iam-authenticator /licenses/aws-iam-authenticator

# Copy kubectl
COPY --from=tools /kubectl /usr/local/bin/kubectl
COPY --from=tools /licenses/kubernetes /licenses/kubernetes

# Copy helm
COPY --from=tools /helm /usr/local/bin/helm
COPY --from=tools /licenses/helm /licenses/helm

# Copy ec2-karpenter-resource-agent
COPY --from=build-src /src/bottlerocket/agents/bin/ec2-karpenter-resource-agent ./
# Copy cloudformation template
COPY --from=build-src /src/bottlerocket/agents/src/bin/ec2-karpenter-resource-agent/cloudformation.yaml /local/cloudformation.yaml
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./ec2-karpenter-resource-agent"]

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

# Copy binary
COPY --from=build-src /src/bottlerocket/agents/bin/vsphere-vm-resource-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./vsphere-vm-resource-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the EKS resource agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as eks-resource-agent
RUN yum install -y unzip
RUN curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip" && \
            unzip awscliv2.zip && \
            ./aws/install

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
# Builds the vSphere K8s cluster resource agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as vsphere-k8s-cluster-resource-agent

RUN yum install -y tar && yum clean all
RUN amazon-linux-extras install -y docker

# Copy eksctl
COPY --from=tools /eksctl /usr/bin/eksctl
COPY --from=tools /licenses/eksctl /licenses/eksctl

# Copy govc
COPY --from=build /usr/libexec/tools/govc /usr/local/bin/govc
COPY --from=build /usr/share/licenses/govmomi /licenses/govmomi

# Copy kubectl
COPY --from=tools /kubectl /usr/local/bin/kubectl
COPY --from=tools /licenses/kubernetes /licenses/kubernetes

# Copy binary
COPY --from=build-src /src/bottlerocket/agents/bin/vsphere-k8s-cluster-resource-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

CMD dockerd --storage-driver vfs &>/dev/null & ./vsphere-k8s-cluster-resource-agent

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the Metal K8s cluster resource agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as metal-k8s-cluster-resource-agent

RUN yum install -y \
        openssh-clients \
        tar \
    && yum clean all
RUN amazon-linux-extras install -y docker

# Copy eksctl
COPY --from=tools /eksctl /usr/bin/eksctl
COPY --from=tools /licenses/eksctl /licenses/eksctl

# Copy kubectl
COPY --from=tools /kubectl /usr/local/bin/kubectl
COPY --from=tools /licenses/kubernetes /licenses/kubernetes

# Copy binary
COPY --from=build-src /src/bottlerocket/agents/bin/metal-k8s-cluster-resource-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

CMD dockerd --storage-driver vfs &>/dev/null & ./metal-k8s-cluster-resource-agent

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the ECS test agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as ecs-test-agent
# Copy binary
COPY --from=build-src /src/bottlerocket/agents/bin/ecs-test-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./ecs-test-agent"]

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the ECS workload test agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 as ecs-workload-agent
# Copy binary
COPY --from=build-src /src/bottlerocket/agents/bin/ecs-workload-agent ./
# Copy licenses
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./ecs-workload-agent"]

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

# =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
# Builds the Kubernetes Workload test agent image
FROM public.ecr.aws/amazonlinux/amazonlinux:2 AS k8s-workload-agent
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

# Copy k8s-workload-agent
COPY --from=build-src /src/bottlerocket/agents/bin/k8s-workload-agent ./
COPY --from=build-src /usr/share/licenses/testsys /licenses/testsys

ENTRYPOINT ["./k8s-workload-agent"]
