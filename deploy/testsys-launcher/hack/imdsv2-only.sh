#!/usr/bin/env bash

# The following is a convenience script that can be used to require IMDSv2

instances=$(aws ec2 describe-instances \
    --filters "Name=tag-value,Values=testsys" "Name=instance-state-code,Values=16" \
    --query "Reservations[*].Instances[*].[InstanceId]" \
    --output text)

for instance in ${instances}; do
    aws ec2 modify-instance-metadata-options \
        --instance-id "${instance}" \
        --http-tokens required \
        --http-endpoint enabled
done

