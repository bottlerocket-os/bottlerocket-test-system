#!/usr/bin/env bash

# The following is a convenience script that can be used to give nodes in the
# "testsys" cluster the "Name" tag.

instances=$(aws ec2 describe-instances \
    --filters "Name=tag-value,Values=testsys" "Name=instance-state-code,Values=16" \
    --query "Reservations[*].Instances[*].[InstanceId]" \
    --output text)

for instance in $instances; do
    aws ec2 create-tags --resources $instance --tags "Key=Name,Value=testsys-node"
    echo "${instance} tagged"
done

