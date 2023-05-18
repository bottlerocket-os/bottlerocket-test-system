# Minimal IAM Permission Map

This doc maps each manifest in [samples](../../samples) to the minimal IAM permissions needed to create and run the test and resources.

The policies can be created using the [aws iam create-policy](https://awscli.amazonaws.com/v2/documentation/api/latest/reference/iam/create-policy.html) command and attached to a role using [aws iam attach-role-policy](https://awscli.amazonaws.com/v2/documentation/api/latest/reference/iam/attach-role-policy.html).

## Without iam:CreateRole Permission

These policies do not include the `iam:CreateRole` permission.

In order for the resources and tests to be created as desired, the ARN of an existing role should be provided instead.

- For ECS clusters, this can be accomplished by adding a field `iamInstanceProfileName` to the ECS cluster config, the value of which is the ARN of a role with at least these permissions: [ecs-iam-instance-profile.json](./ecs-iam-instance-profile.json).

- For EKS clusters, this can be accomplished by replacing the `clusterName`, `region`, and `version` fields with an `encodedConfig` field in the EKS cluster config.
This field's value should be a string representing a base64-encoded EKS cluster config, an example of which can be found [here](./eksctl-config.yaml).
This config should contain the ARNs of an existing service role and an existing node instance role with at least these permissions: [eks-service-role.json](./eks-service-role.json) and [eks-node-instance-role.json](./eks-node-instance-role.json).

### ECS Test

- ecs-test-agent:       [ecs-test-agent.json](./ecs-test-agent.json)
- ecs-resource-agent:   [ecs-resource-agent.json](./ecs-resource-agent-no-create-role.json)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-ecs-cluster.json)

### ECS Migration Test

- ecs-test-agent:       [ecs-test-agent.json](./ecs-test-agent.json)
- migration-test-agent: [migration-test-agent.json](./migration-test-agent-ecs-cluster.json)
- ecs-resource-agent:   [ecs-resource-agent.json](./ecs-resource-agent-no-create-role.json)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-ecs-cluster.json)

### ECS Workload Test

- ecs-workload-agent:   [ecs-workload-agent.json](./ecs-workload-agent.json)
- ecs-resource-agent:   [ecs-resource-agent.json](./ecs-resource-agent-no-create-role.json)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-ecs-cluster.json)

### Sonobuoy Test

- sonobuoy-test-agent:  [sonobuoy-test-agent.json](./sonobuoy-test-agent.json)
- eks-resource-agent:   [eks-resource-agent.json](./eks-resource-agent-no-create-role.json) (if cluster should be created)
                        [eks-resource-agent-existing-cluster.json](./eks-resource-agent-existing-cluster.json) (if cluster already exists)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-eks-cluster.json)

### Sonobuoy Migration Test

- sonobuoy-test-agent:  [sonobuoy-test-agent.json](./sonobuoy-test-agent.json)
- migration-test-agent: [migration-test-agent.json](./migration-test-agent-eks-cluster.json)
- eks-resource-agent:   [eks-resource-agent.json](./eks-resource-agent-no-create-role.json) (if cluster should be created)
                        [eks-resource-agent-existing-cluster.json](./eks-resource-agent-existing-cluster.json) (if cluster already exists)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-eks-cluster.json)

### K8S Workload Test

- k8s-workload-agent:   [k8s-workload-agent.json](./k8s-workload-agent.json)
- eks-resource-agent:   [eks-resource-agent.json](./eks-resource-agent-no-create-role.json) (if cluster should be created)
                        [eks-resource-agent-existing-cluster.json](./eks-resource-agent-existing-cluster.json) (if cluster already exists)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-eks-cluster.json)

## With iam:CreateRole Permission

Some of these policies include the `iam:CreateRole` permission.

_Note_: This is considered dangerous because there is no limit to the permissions and policies that can be assigned to the role created this way, so this new role could end up with `Administrator` privileges.

### ECS Test

- ecs-test-agent:       [ecs-test-agent.json](./ecs-test-agent.json)
- ecs-resource-agent:   [ecs-resource-agent.json](./ecs-resource-agent-create-role.json)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-ecs-cluster.json)

### ECS Migration Test

- ecs-test-agent:       [ecs-test-agent.json](./ecs-test-agent.json)
- migration-test-agent: [migration-test-agent.json](./migration-test-agent-ecs-cluster.json)
- ecs-resource-agent:   [ecs-resource-agent.json](./ecs-resource-agent-create-role.json)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-ecs-cluster.json)

### ECS Workload Test

- ecs-workload-agent:   [ecs-workload-agent.json](./ecs-workload-agent.json)
- ecs-resource-agent:   [ecs-resource-agent.json](./ecs-resource-agent-create-role.json)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-ecs-cluster.json)

### Sonobuoy Test

- sonobuoy-test-agent:  [sonobuoy-test-agent.json](./sonobuoy-test-agent.json)
- eks-resource-agent:   [eks-resource-agent.json](./eks-resource-agent-create-cluster.json) (if cluster should be created)
                        [eks-resource-agent-existing-cluster.json](./eks-resource-agent-existing-cluster.json) (if cluster already exists)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-eks-cluster.json)

### Sonobuoy Migration Test

- sonobuoy-test-agent:  [sonobuoy-test-agent.json](./sonobuoy-test-agent.json)
- migration-test-agent: [migration-test-agent.json](./migration-test-agent-eks-cluster.json)
- eks-resource-agent:   [eks-resource-agent.json](./eks-resource-agent-create-cluster.json) (if cluster should be created)
                        [eks-resource-agent-existing-cluster.json](./eks-resource-agent-existing-cluster.json) (if cluster already exists)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-eks-cluster.json)

### K8S Workload Test

- k8s-workload-agent:   [k8s-workload-agent.json](./k8s-workload-agent.json)
- eks-resource-agent:   [eks-resource-agent.json](./eks-resource-agent-create-cluster.json) (if cluster should be created)
                        [eks-resource-agent-existing-cluster.json](./eks-resource-agent-existing-cluster.json) (if cluster already exists)
- ec2-resource-agent:   [ec2-resource-agent.json](./ec2-resource-agent-eks-cluster.json)
