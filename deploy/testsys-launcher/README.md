# Testsys launcher

The Testsys launcher is an all in one CDK stack for deploying a [Bottlerocket Test System cluster.](https://github.com/bottlerocket-os/bottlerocket-test-system)

## Usage

This will create an EKS cluster with Bottlerocket nodes, the necessary IAM roles
for nodes to assume in order for Testsys to provision additional resources, and
a pre-defined role that can be assumed by an operator.

```sh
cdk deploy
```

The default role that can assume the `testsys-admin` role is "Administrator".
During deployment, use `--parameter TestsysAssumerRole={MY-ROLE-NAME}` to specify
a different role that can assume the `testsys-admin` role.

To get the kubeconfig, assuming you are acting as the role that can assume `testsys-admin`,
use the `aws eks update-kubeconfig` command with the `testsys-admin` role:

```sh
aws eks update-kubeconfig \
    --name testsys \
    --role-arn arn:aws:iam::123456789:role/testsys-admin
```

## Run a sample test

[Refer to `TESTING.md` in the main Bottlerocket repository](https://github.com/bottlerocket-os/bottlerocket/blob/develop/TESTING.md)
for further details on running Bottlerocket tests with Testsys.

## Next steps (managing your testsys cluster)

It is recommended that you install [the Bottlerocket Update Operator](https://github.com/bottlerocket-os/bottlerocket-update-operator)
(or brupop for short) onto your cluster. The brupop Kubernetes controller ensures that the
Bottlerocket nodes on the cluster consume the latest releases and stay up to date.

## Optional Parameters

* `TestsysAssumerRole` name of the role that can assume the `testsys-admin` role to perform cluster operations _(string)_ - Default: "Administrator"
