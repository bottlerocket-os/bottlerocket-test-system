#!/bin/bash

# Get the configuration details for test and save it in a JSON file.
# Set the Task_state for test as running(This can be checked in Kubectl describe test or testsys status).
test-agent-cli init > ExampleConfig.json

echo "starting the Example test"

person=$(jq '.person' ExampleConfig.json)
helloCount=$(jq '.helloCount' ExampleConfig.json)
helloDurationMilliseconds=$(jq '.helloDurationMilliseconds' ExampleConfig.json)

echo "$person"
echo "$helloDurationMilliseconds"
echo "$helloCount"

# Get the number of retries allowed in case of a failing test
retries=$(test-agent-cli retry-count)
echo "no of retires are $retries"

# Perform the test with this bash script.
test () {
    for ((i = 1; i <= helloCount; i++))
    do
        echo "Hello ${i} to ${person}"
        sleep "${helloDurationMilliseconds}"
    done
}

# Retry the test in case of failure
if ! test && [ "$retries" -gt 0 ]; then
    for ((i = 1; i <= "$retries"; i ++))
    do
        echo "Test failed again, retrying ${i} of $retries"
        # This will save the result of current iteration in a directory on kubernetes cluster,
        # that will contain the info how many test cases failed, passed, skipped.
        test-agent-cli send-result -o fail -p 0 -f 1 -s 0

        if test; then
            break
        fi
    done
fi

# Save the test result after consuming all the retries,  that will contain
# the info how many test cases failed, passed, skipped
test-agent-cli send-result -o pass -p 1 -f 0 -s 0

# Set the Task_state as completed. This command will also create the tar file of the results
# This will change the resource status in testsys to passed/failed, but Pod state will remain running if keep_running has been set as true
# To change the pod state to complete set keep_running as false(testsys set test_name --keep-running false)
test-agent-cli terminate
