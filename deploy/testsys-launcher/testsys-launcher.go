package main

import (
	"strings"
	"testsys-launcher/pkg"

	"github.com/aws/aws-cdk-go/awscdk/v2"
	ec2 "github.com/aws/aws-cdk-go/awscdk/v2/awsec2"
	eks "github.com/aws/aws-cdk-go/awscdk/v2/awseks"
	iam "github.com/aws/aws-cdk-go/awscdk/v2/awsiam"
	kubectlLayer "github.com/aws/aws-cdk-go/awscdk/v2/lambdalayerkubectl"
	"github.com/aws/constructs-go/constructs/v10"
	"github.com/aws/jsii-runtime-go"
)

type TestsysLauncherStackProps struct {
	awscdk.StackProps
}

// NewTestsysCluster creates a new EKS 1.25 cluster with the default capacity
// set to 0 and a custom managed nodegroup using bottlerocket AMIs
func NewTestsysCluster(stack constructs.Construct, size float64) eks.Cluster {
	testsysClusterProps := eks.ClusterProps{
		Version:     eks.KubernetesVersion_V1_25(),
		ClusterName: jsii.String("testsys"),
		// This kubectl layer is a lambda layer that can run commands (like
		// applying manifests) for us via the CDK stack
		KubectlLayer: kubectlLayer.NewKubectlLayer(stack, jsii.String("kubectl-lambda-layer")),
		// We don't want to create the cluster with the default node-group using
		// the default optimized AMIs/EC2 instances
		DefaultCapacity: jsii.Number(0),
	}

	// Create the testsys cluster using defined properties
	testsysCluster := eks.NewCluster(stack, jsii.String("testsys"), &testsysClusterProps)

	// Create the role that EC2 nodes can assume
	nodeRole := pkg.NewTestSysNodeRole(stack, "testsys-node-role")

	// Create the testsys bottlerocket node group
	testsysCluster.AddNodegroupCapacity(jsii.String("bottlerocket-nodegroup"), &eks.NodegroupOptions{
		InstanceTypes: &[]ec2.InstanceType{
			ec2.NewInstanceType(jsii.String("m5.xlarge")),
		},
		MinSize:  jsii.Number(size),
		AmiType:  eks.NodegroupAmiType_BOTTLEROCKET_X86_64,
		NodeRole: nodeRole,
	})

	return testsysCluster
}

// NewTestsysAdminUser creates a new "testsys-admin" role and adds it to the
// "masters" list in the Kubernetes cluster aws-auth config map.
// This role can be assumed by the "roleName" that gets passed in.
func NewTestsysAdminUser(stack constructs.Construct, c eks.Cluster, roleNames []string) {
	adminRoleOptions := &iam.FromRoleNameOptions{
		AddGrantsToResources: jsii.Bool(false),
		DefaultPolicyName:    jsii.String("defaultPolicyName"),
		Mutable:              jsii.Bool(false),
	}
	var roles []iam.IPrincipal
	for _, name := range roleNames {
		roles = append(roles, iam.Role_FromRoleName(stack, jsii.String(name), jsii.String(name), adminRoleOptions))
	}

	adminRole := iam.NewRole(stack, jsii.String("testsys-admin"), &iam.RoleProps{
		Description: jsii.String("The admin role for the testsys cluster"),
		AssumedBy:   iam.NewCompositePrincipal(roles...),
		RoleName:    jsii.String("testsys-admin"),
	})

	c.AwsAuth().AddMastersRole(adminRole, jsii.String("admin"))
}

// NewTestsysLauncherStack deploys the entire testsys stack
func NewTestsysLauncherStack(scope constructs.Construct, id string, props *TestsysLauncherStackProps) awscdk.Stack {
	var sprops awscdk.StackProps
	if props != nil {
		sprops = props.StackProps
	}
	stack := awscdk.NewStack(scope, &id, &sprops)

	// Parameters
	var testsysAdminAssumedByContext string = stack.Node().TryGetContext(jsii.String("testsysAdminAssumedBy")).(string)
	testsysAdminAssumedBy := strings.Split(testsysAdminAssumedByContext, ",")

	testsysNodegroupSize := awscdk.NewCfnParameter(stack, jsii.String("TestsysNodegroupSize"), &awscdk.CfnParameterProps{
		Type:        jsii.String("Number"),
		Description: jsii.String("The minimum size of the testsys nodegroup"),
		Default:     jsii.Number(3),
	})

	// Start testsys deployments
	testsysCluster := NewTestsysCluster(stack, *testsysNodegroupSize.ValueAsNumber())
	NewTestsysAdminUser(stack, testsysCluster, testsysAdminAssumedBy)

	return stack
}

func main() {
	defer jsii.Close()

	app := awscdk.NewApp(nil)

	NewTestsysLauncherStack(app, "TestsysLauncherStack", &TestsysLauncherStackProps{
		awscdk.StackProps{
			Env: env(),
		},
	})

	app.Synth(nil)
}

// env determines the AWS environment (account+region) in which our stack is to
// be deployed. For more information see: https://docs.aws.amazon.com/cdk/latest/guide/environments.html
func env() *awscdk.Environment {
	return nil
}
