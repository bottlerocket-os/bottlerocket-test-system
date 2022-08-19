# Limiting IAM Permissions

It is possible to define a role limiting IAM permissions when executing tests against Bottlerocket instances deployed in Amazon EKS or ECS.
It is a best practice to use roles providing the minimum required permissions.
The settings below have been found to be a good basis for assigning these minimum permissions when running `testsys` tests.

## EKS

The following is a minimal set of permissions when deploying to an EKS cluster:

```yaml
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "EC2IG",
      "Effect": "Allow",
      "Action": "ec2:DeleteInternetGateway",
      "Resource": "arn:aws:ec2:*:*:internet-gateway/*"
    },
    {
      "Sid": "ELBIAM",
      "Effect": "Allow",
      "Action": "iam:CreateServiceLinkedRole",
      "Resource": "*",
      "Condition": {
        "StringEquals": {
          "iam:AWSServiceName": "elasticloadbalancing.amazonaws.com"
        }
      }
    },
    {
      "Sid": "IAM",
      "Effect": "Allow",
      "Action": [
        "iam:CreateInstanceProfile",
        "iam:DeleteInstanceProfile",
        "iam:GetRole",
        "iam:GetInstanceProfile",
        "iam:RemoveRoleFromInstanceProfile",
        "iam:CreateRole",
        "iam:DeleteRole",
        "iam:AttachRolePolicy",
        "iam:PutRolePolicy",
        "iam:ListInstanceProfiles",
        "iam:AddRoleToInstanceProfile",
        "iam:ListInstanceProfilesForRole",
        "iam:PassRole",
        "iam:CreateServiceLinkedRole",
        "iam:DetachRolePolicy",
        "iam:DeleteRolePolicy",
        "iam:DeleteServiceLinkedRole",
        "iam:GetRolePolicy",
        "iam:ListAttachedRolePolicies"
      ],
      "Resource": [
        "arn:aws:iam::*:instance-profile/eksctl-*",
        "arn:aws:iam::*:role/eksctl-*",
        "arn:aws:iam::*:role/aws-service-role/eks.amazonaws.com/*",
        "arn:aws:iam::*:role/aws-service-role/eks-nodegroup.amazonaws.com/*"
      ]
    },
    {
      "Sid": "IAMOIDC",
      "Effect": "Allow",
      "Action": "iam:GetOpenIDConnectProvider",
      "Resource": "arn:aws:iam::*:oidc-provider/oidc.eks.*.amazonaws.com/*"
    },
    {
      "Sid": "EC2",
      "Effect": "Allow",
      "Action": [
        "ec2:AuthorizeSecurityGroupIngress",
        "ec2:DescribeInstances",
        "ec2:AttachInternetGateway",
        "ec2:DeleteRouteTable",
        "ec2:RevokeSecurityGroupEgress",
        "ec2:CreateRoute",
        "ec2:CreateInternetGateway",
        "ec2:DescribeVolumes",
        "ec2:DeleteInternetGateway",
        "ec2:DescribeKeyPairs",
        "ec2:ImportKeyPair",
        "ec2:CreateTags",
        "ec2:RunInstances",
        "ec2:DisassociateRouteTable",
        "ec2:CreateVolume",
        "ec2:RevokeSecurityGroupIngress",
        "ec2:DescribeImageAttribute",
        "ec2:DeleteNatGateway",
        "ec2:CreateSubnet",
        "ec2:DescribeSubnets",
        "ec2:AttachVolume",
        "ec2:CreateNatGateway",
        "ec2:CreateVpc",
        "ec2:DescribeVpcAttribute",
        "ec2:ModifySubnetAttribute",
        "ec2:DescribeAvailabilityZones",
        "ec2:ReleaseAddress",
        "ec2:DeleteLaunchTemplate",
        "ec2:DescribeSecurityGroups",
        "ec2:CreateLaunchTemplate",
        "ec2:DescribeVpcs",
        "ec2:DeleteSubnet",
        "ec2:DescribeVolumesModifications",
        "ec2:AssociateRouteTable",
        "ec2:DescribeInternetGateways",
        "ec2:DeleteVolume",
        "ec2:DescribeAccountAttributes",
        "ec2:DescribeRouteTables",
        "ec2:DetachVolume",
        "ec2:ModifyVolume",
        "ec2:DescribeLaunchTemplates",
        "ec2:CreateRouteTable",
        "ec2:DetachInternetGateway",
        "ec2:DeleteVpc",
        "ec2:DescribeAddresses",
        "ec2:DeleteTags",
        "ec2:DescribeDhcpOptions",
        "ec2:DescribeNetworkInterfaces",
        "ec2:CreateSecurityGroup",
        "ec2:ModifyVpcAttribute",
        "ec2:ModifyInstanceAttribute",
        "ec2:AuthorizeSecurityGroupEgress",
        "ec2:DescribeTags",
        "ec2:DeleteRoute",
        "ec2:DescribeLaunchTemplateVersions",
        "ec2:DescribeNatGateways",
        "ec2:AllocateAddress",
        "ec2:DescribeImages",
        "ec2:DeleteSecurityGroup"
      ],
      "Resource": "*"
    },
    {
      "Sid": "ELB",
      "Effect": "Allow",
      "Action": [
        "elasticloadbalancing:ModifyListener",
        "elasticloadbalancing:SetLoadBalancerPoliciesForBackendServer",
        "elasticloadbalancing:CreateTargetGroup",
        "elasticloadbalancing:AddTags",
        "elasticloadbalancing:DeleteLoadBalancerListeners",
        "elasticloadbalancing:ModifyLoadBalancerAttributes",
        "elasticloadbalancing:CreateLoadBalancerPolicy",
        "elasticloadbalancing:CreateLoadBalancer",
        "elasticloadbalancing:DeleteTargetGroup",
        "elasticloadbalancing:SetLoadBalancerPoliciesOfListener",
        "elasticloadbalancing:DescribeTargetGroups",
        "elasticloadbalancing:DeleteListener",
        "elasticloadbalancing:DetachLoadBalancerFromSubnets",
        "elasticloadbalancing:RegisterTargets",
        "elasticloadbalancing:DeleteLoadBalancer",
        "elasticloadbalancing:DescribeLoadBalancers",
        "elasticloadbalancing:DescribeLoadBalancerPolicies",
        "elasticloadbalancing:ModifyTargetGroupAttributes",
        "elasticloadbalancing:DeregisterInstancesFromLoadBalancer",
        "elasticloadbalancing:RegisterInstancesWithLoadBalancer",
        "elasticloadbalancing:DeregisterTargets",
        "elasticloadbalancing:DescribeLoadBalancerAttributes",
        "elasticloadbalancing:DescribeTargetGroupAttributes",
        "elasticloadbalancing:ConfigureHealthCheck",
        "elasticloadbalancing:CreateListener",
        "elasticloadbalancing:DescribeListeners",
        "elasticloadbalancing:ApplySecurityGroupsToLoadBalancer",
        "elasticloadbalancing:AttachLoadBalancerToSubnets",
        "elasticloadbalancing:CreateLoadBalancerListeners",
        "elasticloadbalancing:DescribeTargetHealth",
        "elasticloadbalancing:ModifyTargetGroup"
      ],
      "Resource": "*"
    },
    {
      "Sid": "ECR",
      "Effect": "Allow",
      "Action": [
        "ecr:GetAuthorizationToken",
        "ecr:InitiateLayerUpload",
        "ecr:ListImages",
        "ecr:BatchCheckLayerAvailability",
        "ecr:GetDownloadUrlForLayer",
        "ecr:PutImage",
        "ecr:BatchGetImage",
        "ecr:DescribeImages",
        "ecr:UploadLayerPart",
        "ecr:CompleteLayerUpload",
        "ecr:DescribeRepositories"
      ],
      "Resource": "*"
    },
    {
      "Sid": "AUTOSCALING",
      "Effect": "Allow",
      "Action": [
        "autoscaling:DeleteAutoScalingGroup",
        "autoscaling:DescribeScalingActivities",
        "autoscaling:CreateLaunchConfiguration",
        "autoscaling:DescribeAutoScalingGroups",
        "autoscaling:UpdateAutoScalingGroup",
        "autoscaling:CreateAutoScalingGroup",
        "autoscaling:DescribeLaunchConfigurations",
        "autoscaling:DeleteLaunchConfiguration"
      ],
      "Resource": "*"
    },
    {
      "Sid": "SSM",
      "Effect": "Allow",
      "Action": [
        "ssm:GetParametersByPath",
        "ssm:GetParameter",
        "ssm:DeleteParameter",
        "ssm:DescribeParameters",
        "ssm:GetParameters",
        "ssm:DeleteParameters",
        "ssm:PutParameter",
        "ssm:GetParameterHistory"
      ],
      "Resource": "*"
    },
    {
      "Sid": "Full",
      "Effect": "Allow",
      "Action": ["cloudformation:*", "eks:*"],
      "Resource": "*"
    },
    {
      "Sid": "KMS",
      "Effect": "Allow",
      "Action": ["kms:DescribeKey"],
      "Resource": "*"
    },
    {
      "Sid": "IAMGetRole",
      "Effect": "Allow",
      "Action": ["iam:GetRole"],
      "Resource": "*"
    }
  ]
}
```

## ECS

The following is a minimal set of permissions when deploying to ECS:

```text
Details coming soon.
```
