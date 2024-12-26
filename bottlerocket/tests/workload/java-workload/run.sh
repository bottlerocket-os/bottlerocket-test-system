#!/bin/bash

CLUSTER_NAME="java-test"
REGION="us-west-2"
EKS_CONFIG_FILE="./eks-config.yaml"
POD_CONFIG_FILE="./java-test.pod.yaml"
RESULTS_DIR="${results_dir}"
KUBECONFIG_PATH="./${CLUSTER_NAME}.kubeconfig"
ERROR_LOG="${RESULTS_DIR}/error.log"
AMI_ID=$(aws ssm get-parameters --names /aws/service/bottlerocket/aws-k8s-1.30/x86_64/latest/image_id --query 'Parameters[0].Value' --output text)

mkdir -p $RESULTS_DIR || { echo "Failed to create directory $RESULTS_DIR"; exit 1; }

log_error() {
    echo "$(date): $1" >> $ERROR_LOG
}

if [ -z "$AMI_ID" ]; then
    log_error "Failed to retrieve AMI ID"
    exit 1
fi

# Update eks-config.yaml with the new AMI ID
sed -i "s/^[[:space:]]*ami:.*$/  ami: $AMI_ID/" $EKS_CONFIG_FILE || { log_error "Failed to update AMI ID in eks-config.yaml"; exit 1; }

# Get AWS Session Token
echo "Getting AWS Session Token..."
AWS_SESSION_TOKEN=$(aws sts get-session-token --duration-seconds 3600 --query 'Credentials.[AccessKeyId, SecretAccessKey, SessionToken]' --output text | awk '{print $3}')
if [ -z "$AWS_SESSION_TOKEN" ]; then
    log_error "Failed to retrieve AWS Session Token"
    exit 1
fi

# Step 1: Create EKS Cluster
echo "Creating EKS Cluster..."
eksctl create cluster -f $EKS_CONFIG_FILE || { log_error "Failed to create EKS cluster"; exit 1; }

# Step 2: Update kubeconfig
echo "Updating kubeconfig..."
aws eks update-kubeconfig --region $REGION --name $CLUSTER_NAME --kubeconfig $KUBECONFIG_PATH || { log_error "Failed to update kubeconfig"; exit 1; }

# Step 3: Apply the pod configuration to the cluster
echo "Applying pod configuration..."
kubectl apply -f $POD_CONFIG_FILE --kubeconfig $KUBECONFIG_PATH || { log_error "Failed to apply pod configuration"; exit 1; }

# Step 4: Wait for the pod to be running
echo "Waiting for pod to be ready..."
kubectl wait --for=condition=Ready pod/wildfly-pod --kubeconfig $KUBECONFIG_PATH --timeout=300s || { log_error "Pod did not become ready in time"; exit 1; }

# Step 5: Retrieve logs from the pod
echo "Fetching logs..."
kubectl logs wildfly-pod --kubeconfig $KUBECONFIG_PATH > "${RESULTS_DIR}/${CLUSTER_NAME}-testsys.log" || { log_error "Failed to retrieve logs"; exit 1; }

echo "Process completed successfully. Logs saved in ${RESULTS_DIR}/${CLUSTER_NAME}-testsys.log"