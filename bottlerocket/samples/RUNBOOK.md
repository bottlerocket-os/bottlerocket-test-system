# Bottlerocket Test System Sample Files

These files contain templates that can be populated using environment variables and the `eval` command and run with the `cli` tool.

Examples of how to populate each one of these files can be found below.

Each codeblock starts with a small number of variables that need to be populated before running the block, followed by a number of variables that contain default values (but can be modified by the user).

_Note_: `ASSUME_ROLE` has a default value of `~` (null), but you can replace this with the ARN of an AWS IAM role that should be used for all AWS calls.

The final `cat` command will print the populated file to the path indicated by `OUTPUT_FILE`.

## EKS

The files in [eks](./eks) are meant to be run on an EKS test cluster. You can create a new cluster using the [eksctl](https://eksctl.io/introduction/) tool.

### Migration Testing on `aws-ecs` Variants

```bash
CLUSTER_NAME="x86-64-aws-ecs-1"
OUTPUT_FILE="${CLUSTER_NAME}-migration.yaml"
VARIANT="aws-ecs-1"
ARCHITECTURE="x86_64"
AGENT_IMAGE_VERSION=$(cli --version | sed -e "s/^.* //g")
ECS_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ecs-test-agent:v${AGENT_IMAGE_VERSION}"
MIGRATION_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/migration-test-agent:v${AGENT_IMAGE_VERSION}"
ECS_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ecs-resource-agent:v${AGENT_IMAGE_VERSION}"
EC2_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ec2-resource-agent:v${AGENT_IMAGE_VERSION}"
ASSUME_ROLE="~"
AWS_REGION="us-west-2"
UPGRADE_VERSION="v1.11.1"
STARTING_VERSION="v1.11.0"
METADATA_URL="https://updates.bottlerocket.aws/2020-07-07/${VARIANT}/${ARCHITECTURE}"
TARGETS_URL="https://updates.bottlerocket.aws/targets"

BOTTLEROCKET_AMI_ID=$(aws ssm get-parameter \
  --region ${AWS_REGION} \
  --name "/aws/service/bottlerocket/${VARIANT}/${ARCHITECTURE}/$(echo ${STARTING_VERSION} | tr -d "v")/image_id" \
  --query Parameter.Value --output text)

eval "cat > ${OUTPUT_FILE} << EOF
$(< eks/ecs-migration-test.yaml)
EOF
" 2> /dev/null
```

### Conformance Testing on `aws-ecs` Variants

```bash
CLUSTER_NAME="x86-64-aws-ecs-1"
OUTPUT_FILE="${CLUSTER_NAME}.yaml"
VARIANT="aws-ecs-1"
ARCHITECTURE="x86_64"
AGENT_IMAGE_VERSION=$(cli --version | sed -e "s/^.* //g")
ECS_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ecs-resource-agent:v${AGENT_IMAGE_VERSION}"
EC2_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ec2-resource-agent:v${AGENT_IMAGE_VERSION}"
ECS_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ecs-test-agent:v${AGENT_IMAGE_VERSION}"
ASSUME_ROLE="~"
AWS_REGION="us-west-2"

BOTTLEROCKET_AMI_ID=$(aws ssm get-parameter \
  --region ${AWS_REGION} \
  --name "/aws/service/bottlerocket/${VARIANT}/${ARCHITECTURE}/latest/image_id" \
  --query Parameter.Value --output text)

eval "cat > ${OUTPUT_FILE} << EOF
$(< eks/ecs-test.yaml)
EOF
" 2> /dev/null
```

### Migration Testing on `aws-k8s` Variants

```bash
CLUSTER_NAME="x86-64-aws-k8s-124"
OUTPUT_FILE="${CLUSTER_NAME}-migration.yaml"
VARIANT="aws-k8s-1.24"
ARCHITECTURE="x86_64"
AGENT_IMAGE_VERSION=$(cli --version | sed -e "s/^.* //g")
SONOBUOY_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/sonobuoy-test-agent:v${AGENT_IMAGE_VERSION}"
MIGRATION_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/migration-test-agent:v${AGENT_IMAGE_VERSION}"
EKS_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/eks-resource-agent:v${AGENT_IMAGE_VERSION}"
EC2_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ec2-resource-agent:v${AGENT_IMAGE_VERSION}"
ASSUME_ROLE="~"
AWS_REGION="us-west-2"
UPGRADE_VERSION="v1.11.1"
STARTING_VERSION="v1.11.0"
METADATA_URL="https://updates.bottlerocket.aws/2020-07-07/${VARIANT}/${ARCHITECTURE}"
TARGETS_URL="https://updates.bottlerocket.aws/targets"

BOTTLEROCKET_AMI_ID=$(aws ssm get-parameter \
  --region ${AWS_REGION} \
  --name "/aws/service/bottlerocket/${VARIANT}/${ARCHITECTURE}/$(echo ${STARTING_VERSION} | tr -d "v")/image_id" \
  --query Parameter.Value --output text)

eval "cat > ${OUTPUT_FILE} << EOF
$(< eks/sonobuoy-migration-test.yaml)
EOF
" 2> /dev/null
```

### Conformance Testing on `aws-k8s` Variants

```bash
CLUSTER_NAME="x86-64-aws-k8s-124"
OUTPUT_FILE="${CLUSTER_NAME}.yaml"
VARIANT="aws-k8s-1.24"
ARCHITECTURE="x86_64"
AGENT_IMAGE_VERSION=$(cli --version | sed -e "s/^.* //g")
SONOBUOY_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/sonobuoy-test-agent:v${AGENT_IMAGE_VERSION}"
EKS_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/eks-resource-agent:v${AGENT_IMAGE_VERSION}"
EC2_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ec2-resource-agent:v${AGENT_IMAGE_VERSION}"
ASSUME_ROLE="~"
AWS_REGION="us-west-2"
SONOBUOY_MODE="quick"

BOTTLEROCKET_AMI_ID=$(aws ssm get-parameter \
  --region ${AWS_REGION} \
  --name "/aws/service/bottlerocket/${VARIANT}/${ARCHITECTURE}/latest/image_id" \
  --query Parameter.Value --output text)

eval "cat > ${OUTPUT_FILE} << EOF
$(< eks/sonobuoy-test.yaml)
EOF
" 2> /dev/null
```

### Migration Testing on `vmware-k8s` Variants

This codeblock assumes that your vSphere config file has been sourced. Specifically, the variables `GOVC_USERNAME`, `GOVC_PASSWORD`, `GOVC_DATACENTER`, `GOVC_DATASTORE`, `GOVC_URL`, `GOVC_NETWORK`, `GOVC_RESOURCE_POOL`, and `GOVC_FOLDER` need to be populated.

```bash
CONTROL_PLANE_ENDPOINT_IP=
MGMT_CLUSTER_KUBECONFIG_PATH=

CLUSTER_NAME="vmware-k8s-124"
OUTPUT_FILE="${CLUSTER_NAME}-migration.yaml"
VARIANT="vmware-k8s-1.24"
K8S_VERSION="1.24"
AGENT_IMAGE_VERSION=$(cli --version | sed -e "s/^.* //g")
SONOBUOY_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/sonobuoy-test-agent:v${AGENT_IMAGE_VERSION}"
MIGRATION_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/migration-test-agent:v${AGENT_IMAGE_VERSION}"
VSPHERE_K8S_CLUSTER_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/vsphere-k8s-cluster-resource-agent:v${AGENT_IMAGE_VERSION}"
VSPHERE_VM_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/vsphere-vm-resource-agent:v${AGENT_IMAGE_VERSION}"
ASSUME_ROLE="~"
REGION="us-west-2"
UPGRADE_VERSION="v1.11.1"
STARTING_VERSION="v1.11.0"
METADATA_URL="https://updates.bottlerocket.aws/2020-07-07/${VARIANT}/x86_64"
TARGETS_URL="https://updates.bottlerocket.aws/targets"
OVA_NAME="bottlerocket-${VARIANT}-x86_64-${STARTING_VERSION}.ova"
MGMT_CLUSTER_KUBECONFIG_BASE64=$(cat ${MGMT_CLUSTER_KUBECONFIG_PATH} | base64)

cli add-secret map  \
 --name "vsphere-creds" \
 "username=${GOVC_USERNAME}" \
 "password=${GOVC_PASSWORD}"

eval "cat > ${OUTPUT_FILE} << EOF
$(< eks/vmware-migration-test.yaml)
EOF
" 2> /dev/null
```

### Conformance Testing on `vmware-k8s` Variants

This codeblock assumes that your vSphere config file has been sourced. Specifically, the variables `GOVC_USERNAME`, `GOVC_PASSWORD`, `GOVC_DATACENTER`, `GOVC_DATASTORE`, `GOVC_URL`, `GOVC_NETWORK`, `GOVC_RESOURCE_POOL`, and `GOVC_FOLDER` need to be populated.

```bash
CONTROL_PLANE_ENDPOINT_IP=
MGMT_CLUSTER_KUBECONFIG_PATH=

CLUSTER_NAME="vmware-k8s-124"
OUTPUT_FILE="${CLUSTER_NAME}.yaml"
VARIANT="vmware-k8s-1.24"
K8S_VERSION="1.24"
AGENT_IMAGE_VERSION=$(cli --version | sed -e "s/^.* //g")
SONOBUOY_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/sonobuoy-test-agent:v${AGENT_IMAGE_VERSION}"
VSPHERE_K8S_CLUSTER_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/vsphere-k8s-cluster-resource-agent:v${AGENT_IMAGE_VERSION}"
VSPHERE_VM_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/vsphere-vm-resource-agent:v${AGENT_IMAGE_VERSION}"
ASSUME_ROLE="~"
REGION="us-west-2"
SONOBUOY_MODE="quick"
VERSION="v1.11.1"
METADATA_URL="https://updates.bottlerocket.aws/2020-07-07/${VARIANT}/x86_64"
TARGETS_URL="https://updates.bottlerocket.aws/targets"
OVA_NAME="bottlerocket-${VARIANT}-x86_64-${VERSION}.ova"
MGMT_CLUSTER_KUBECONFIG_BASE64=$(cat $MGMT-CLUSTER-KUBECONFIG-PATH | base64)

cli add-secret map  \
 --name "vsphere-creds" \
 "username=${GOVC_USERNAME}" \
 "password=${GOVC_PASSWORD}"

eval "cat > ${OUTPUT_FILE} << EOF
$(< eks/vmware-sonobuoy-test.yaml)
EOF
" 2> /dev/null
```

## kind

The files in [kind](./kind) are meant to be run on a `kind` cluster. Directions on how to use a `kind` cluster with TestSys can be found in our [QUICKSTART](../../docs/QUICKSTART.md).

### Conformance Testing on `aws-ecs` Variants

```bash
CLUSTER_NAME="x86-64-aws-ecs-1"
OUTPUT_FILE="${CLUSTER_NAME}.yaml"
VARIANT="aws-ecs-1"
ARCHITECTURE="x86_64"
AGENT_IMAGE_VERSION=$(cli --version | sed -e "s/^.* //g")
ACCESS_KEY_ID=$(aws configure get aws_access_key_id)
SECRET_ACCESS_KEY=$(aws configure get aws_secret_access_key)
ECS_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ecs-test-agent:v${AGENT_IMAGE_VERSION}"
ECS_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ecs-resource-agent:v${AGENT_IMAGE_VERSION}"
EC2_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ec2-resource-agent:v${AGENT_IMAGE_VERSION}"
ASSUME_ROLE="~"
AWS_REGION="us-west-2"

BOTTLEROCKET_AMI_ID=$(aws ssm get-parameter \
  --region ${AWS_REGION} \
  --name "/aws/service/bottlerocket/${VARIANT}/${ARCHITECTURE}/latest/image_id" \
  --query Parameter.Value --output text)

cli add-secret map  \
 --name "aws-creds" \
 "ACCESS_KEY_ID=${ACCESS_KEY_ID}" \
 "SECRET_ACCESS_KEY=${SECRET_ACCESS_KEY}"

eval "cat > ${OUTPUT_FILE} << EOF
$(< kind/ecs-test.yaml)
EOF
" 2> /dev/null
```

### Conformance Testing on `aws-k8s` Variants

```bash
CLUSTER_NAME="x86-64-aws-k8s-124"
OUTPUT_FILE="${CLUSTER_NAME}.yaml"
VARIANT="aws-k8s-1.24"
ARCHITECTURE="x86_64"
AGENT_IMAGE_VERSION=$(cli --version | sed -e "s/^.* //g")
ACCESS_KEY_ID=$(aws configure get aws_access_key_id)
SECRET_ACCESS_KEY=$(aws configure get aws_secret_access_key)
SONOBUOY_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/sonobuoy-test-agent:v${AGENT_IMAGE_VERSION}"
EKS_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/eks-resource-agent:v${AGENT_IMAGE_VERSION}"
EC2_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/ec2-resource-agent:v${AGENT_IMAGE_VERSION}"
ASSUME_ROLE="~"
AWS_REGION="us-west-2"
SONOBUOY_MODE="quick"

BOTTLEROCKET_AMI_ID=$(aws ssm get-parameter \
  --region ${AWS_REGION} \
  --name "/aws/service/bottlerocket/${VARIANT}/${ARCHITECTURE}/latest/image_id" \
  --query Parameter.Value --output text)

cli add-secret map  \
 --name "aws-creds" \
 "ACCESS_KEY_ID=${ACCESS_KEY_ID}" \
 "SECRET_ACCESS_KEY=${SECRET_ACCESS_KEY}"

eval "cat > ${OUTPUT_FILE} << EOF
$(< kind/sonobuoy-test.yaml)
EOF
" 2> /dev/null
```

### Conformance Testing on `vmware-k8s` Variants

This codeblock assumes that your vSphere config file has been sourced. Specifically, the variables `GOVC_USERNAME`, `GOVC_PASSWORD`, `GOVC_DATACENTER`, `GOVC_DATASTORE`, `GOVC_URL`, `GOVC_NETWORK`, `GOVC_RESOURCE_POOL`, and `GOVC_FOLDER` need to be populated.

```bash
CONTROL_PLANE_ENDPOINT_IP=
MGMT_CLUSTER_KUBECONFIG_PATH=

CLUSTER_NAME="vmware-k8s-124"
OUTPUT_FILE="${CLUSTER_NAME}.yaml"
VARIANT="vmware-k8s-1.24"
K8S_VERSION="1.24"
AGENT_IMAGE_VERSION=$(cli --version | sed -e "s/^.* //g")
SONOBUOY_TEST_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/sonobuoy-test-agent:v${AGENT_IMAGE_VERSION}"
VSPHERE_K8S_CLUSTER_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/vsphere-k8s-cluster-resource-agent:v${AGENT_IMAGE_VERSION}"
VSPHERE_VM_RESOURCE_AGENT_IMAGE_URI="public.ecr.aws/bottlerocket-test-system/vsphere-vm-resource-agent:v${AGENT_IMAGE_VERSION}"
ASSUME_ROLE="~"
REGION="us-west-2"
SONOBUOY_MODE="quick"
VERSION="v1.11.1"
METADATA_URL="https://updates.bottlerocket.aws/2020-07-07/${VARIANT}/x86_64"
TARGETS_URL="https://updates.bottlerocket.aws/targets"
OVA_NAME="bottlerocket-${VARIANT}-x86_64-${VERSION}.ova"
MGMT_CLUSTER_KUBECONFIG_BASE64=$(cat ${MGMT_CLUSTER_KUBECONFIG_PATH} | base64)

cli add-secret map  \
 --name "vsphere-creds" \
 "username=$GOVC_USERNAME" \
 "password=$GOVC_PASSWORD"

eval "cat > ${OUTPUT_FILE} << EOF
$(< kind/vmware-sonobuoy-test.yaml)
EOF
" 2> /dev/null
```
