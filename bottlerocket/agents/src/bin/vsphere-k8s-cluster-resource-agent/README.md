# vSphere K8s Cluster resource agent

This TestSys resource agent is responsible for provisioning vSphere K8s clusters via Cluster-API vSphere using [EKS Anywhere](https://github.com/aws/eks-anywhere).

## Example configuration for the resource agent

```yaml
apiVersion: testsys.system/v1
kind: Resource
metadata:
  name: my-vsphere-cluster
  namespace: testsys
spec:
  agent:
    name: vsphere-k8s-cluster-resource-agent
    image: <vsphere-k8s-cluster-resource-agent-image>
    keepRunning: false
    privileged: true
    timeout: 20d
    secrets:
      vsphereCredentials: <K8s secret storing username/password for vsphere API>
    configuration:
      name: br-eksa-123
      controlPlaneEndpointIp: <IP to allocate for the cluster control plane endpoint>
      creation_policy: IfNotExists
      version: v1.23
      ovaName: <name of the Bottlerocket OVA to import, e.g. "bottlerocket-vmware-k8s-1.23-x86_64-v1.10.1.ova">
      tufRepo:
        metadataUrl: "https://updates.bottlerocket.aws/2020-07-07/vmware-k8s-1.23/x86_64/"
        targetsUrl: "https://updates.bottlerocket.aws/targets"
      vcenterHostUrl: <vCenter host uRL>
      vcenterDatacenter: <vCenter datacenter>
      vcenterDatastore: <vCenter datastore>
      vcenterNetwork: <vCenter network>
      vcenterResourcePool: <vCenter resource pool>
      vcenterWorkloadFolder: <vCenter workload folder>
      mgmtClusterKubeconfigBase64: <Base64-encoded kubeconfig for the CAPI management cluster used to deploy the vSphere cluster>
      destructionPolicy: OnDeletion
```

## How to set up EKS Anywhere cluster for use as management cluster

EKS Anywhere recommends using a separate management cluster for managing different vSphere clusters.
EKS Anywhere's "Getting Started" instructions have a [section on setting up an initial management cluster](https://anywhere.eks.amazonaws.com/docs/getting-started/production-environment/vsphere-getstarted/#create-an-initial-cluster).

Once the management cluster created, you can base64-encode the kubeconfig for the management cluster and plug the value in the `mgmtClusterKubeconfigBase64` field of the Resource agent configuration.
