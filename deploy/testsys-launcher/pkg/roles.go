package pkg

import (
	iam "github.com/aws/aws-cdk-go/awscdk/v2/awsiam"
	"github.com/aws/constructs-go/constructs/v10"
	"github.com/aws/jsii-runtime-go"
)

// NewTestSysNodeRole creates a new role that testsys cluster nodes can assume
// to perform ALL testsys operations (create resources, perform migration tests, etc.)
// For a full reconciliation on these IAM permissions, see:
// - https://github.com/bottlerocket-os/bottlerocket-test-system/pull/775
// - https://eksctl.io/usage/iamserviceaccounts/
func NewTestSysNodeRole(stack constructs.Construct, roleName string) iam.Role {
	nodePolicies := iam.NewPolicyDocument(&iam.PolicyDocumentProps{
		Statements: &[]iam.PolicyStatement{
			iam.NewPolicyStatement(
				&iam.PolicyStatementProps{
					Effect: iam.Effect_ALLOW,
					Actions: &[]*string{
						// ECS permissions so testsys can manage and provision
						// ECS variant tests and clusters
						jsii.String("ecs:CreateCluster"),
						jsii.String("ecs:DeleteCluster"),
						jsii.String("ecs:DeregisterContainerInstance"),
						jsii.String("ecs:DescribeClusters"),
						jsii.String("ecs:DescribeTaskDefinition"),
						jsii.String("ecs:DescribeTasks"),
						jsii.String("ecs:DiscoverPoolEndpoint"),
						jsii.String("ecs:ListContainerInstances"),
						jsii.String("ecs:ListTaskDefinitions"),
						jsii.String("ecs:RegisterContainerInstance"),
						jsii.String("ecs:RunTask"),
						jsii.String("ecs:SubmitTaskStateChange"),

						// EKS all access permissions (to create, delete, tag, etc.
						jsii.String("eks:*"),

						// IAM permissions so testsys can manage roles for
						// resources that it creates (like k8s clusters through
						// eksctl)
						jsii.String("iam:AddRoleToInstanceProfile"),
						jsii.String("iam:AttachRolePolicy"),
						jsii.String("iam:CreateInstanceProfile"),
						jsii.String("iam:CreateOpenIDConnectProvider"),
						jsii.String("iam:CreateRole"),
						jsii.String("iam:DeleteInstanceProfile"),
						jsii.String("iam:DeleteOpenIDConnectProvider"),
						jsii.String("iam:DeleteRole"),
						jsii.String("iam:DeleteRolePolicy"),
						jsii.String("iam:DetachRolePolicy"),
						jsii.String("iam:GetInstanceProfile"),
						jsii.String("iam:GetOpenIDConnectProvider"),
						jsii.String("iam:GetRole"),
						jsii.String("iam:GetRolePolicy"),
						jsii.String("iam:ListInstanceProfilesForRole"),
						jsii.String("iam:PassRole"),
						jsii.String("iam:PutRolePolicy"),
						jsii.String("iam:RemoveRoleFromInstanceProfile"),

						// Aws sts permissions
						jsii.String("sts:GetCallerIdentity"),
						jsii.String("sts:AssumeRole"),
					},
					Resources: &[]*string{
						jsii.String("*"),
					},
				}),
		},
	},
	)

	nodeRole := iam.NewRole(stack, jsii.String("testsys-node-role"), &iam.RoleProps{
		Description: jsii.String("The testsys cluster"),
		AssumedBy:   iam.NewServicePrincipal(jsii.String("ec2.amazonaws.com"), &iam.ServicePrincipalOpts{}),
		RoleName:    jsii.String(roleName),
		InlinePolicies: &map[string]iam.PolicyDocument{
			"testsys-node-permissions": nodePolicies,
		},
	})

	// Role needed to create EC2 resources. What eksctl creates upon cluster provisioning
	nodeRole.AddManagedPolicy(iam.ManagedPolicy_FromAwsManagedPolicyName(jsii.String("AmazonEC2FullAccess")))

	// SSM access in order to
	nodeRole.AddManagedPolicy(iam.ManagedPolicy_FromAwsManagedPolicyName(jsii.String("AmazonSSMFullAccess")))

	// Readonly role for getting container images
	nodeRole.AddManagedPolicy(iam.ManagedPolicy_FromAwsManagedPolicyName(jsii.String("AmazonEC2ContainerRegistryReadOnly")))

	// Container networking interface role required by EKS clusters
	nodeRole.AddManagedPolicy(iam.ManagedPolicy_FromAwsManagedPolicyName(jsii.String("AmazonEKS_CNI_Policy")))

	// EKS worker node role required by EKS clusters
	nodeRole.AddManagedPolicy(iam.ManagedPolicy_FromAwsManagedPolicyName(jsii.String("AmazonEKSWorkerNodePolicy")))

	// Required for testsys (through eksctl) to create/manage cloudformation stacks
	nodeRole.AddManagedPolicy(iam.ManagedPolicy_FromAwsManagedPolicyName(jsii.String("AWSCloudFormationFullAccess")))

	return nodeRole
}
