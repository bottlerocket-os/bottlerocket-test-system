ARG UNAME_ARCH
FROM bottlerocket-sdk-openssl-${UNAME_ARCH} as build
ARG UNAME_ARCH
USER root

ADD ./ /src/
WORKDIR /src/agent/sonobuoy-test-agent
RUN cargo install --locked --target ${UNAME_ARCH}-bottlerocket-linux-musl --path . --root ./

FROM public.ecr.aws/amazonlinux/amazonlinux:2
ARG ARCH
ARG SONOBUOY_VERSION=0.53.2
ARG SONOBUOY_URL=https://github.com/vmware-tanzu/sonobuoy/releases/download/v${SONOBUOY_VERSION}/sonobuoy_${SONOBUOY_VERSION}_linux_${ARCH}.tar.gz
ARG AWS_IAM_AUTHENTICATOR_URL=https://amazon-eks.s3.us-west-2.amazonaws.com/1.21.2/2021-07-05/bin/linux/${ARCH}/aws-iam-authenticator
RUN yum install -y tar gzip && yum clean all

# Download aws-iam-authenticator
RUN temp_dir="$(mktemp -d --suffix aws-iam-authenticator)" && \
    curl -fsSL "${AWS_IAM_AUTHENTICATOR_URL}" -o "${temp_dir}/${AWS_IAM_AUTHENTICATOR_URL##*/}" && \
    chmod 0755 "${temp_dir}/${AWS_IAM_AUTHENTICATOR_URL##*/}" && \
    mv "${temp_dir}/${AWS_IAM_AUTHENTICATOR_URL##*/}" /usr/bin/aws-iam-authenticator && \
    rm -rf ${temp_dir}

# Download sonobuoy
RUN temp_dir="$(mktemp -d --suffix sonobuoy-setup)" && \
    curl -fsSL "${SONOBUOY_URL}" -o "${temp_dir}/${SONOBUOY_URL##*/}" && \
    tar xpf "${temp_dir}/${SONOBUOY_URL##*/}" -C "${temp_dir}" sonobuoy && \
    chmod 0755 "${temp_dir}/sonobuoy" && \
    mv "${temp_dir}/sonobuoy" /usr/bin/sonobuoy && \
    rm -rf ${temp_dir}
# Copy binary
COPY --from=build /src/agent/sonobuoy-test-agent/bin/sonobuoy-test-agent ./

ENTRYPOINT ["./sonobuoy-test-agent"]
