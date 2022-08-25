#!/bin/bash

# The config structure is as follows:
#   person: String
#   helloCount: Integer
#   helloDurationMilliseconds: Integer

# Store the configuration details in a JSON file and mark the test as Running
test-agent-cli init > ExampleConfig.json

echo "starting the Example test"

person=$(jq '.person' ExampleConfig.json)
helloCount=$(jq '.helloCount' ExampleConfig.json)
helloDurationMilliseconds=$(jq '.helloDurationMilliseconds' ExampleConfig.json)

echo "${person}"
echo "${helloDurationMilliseconds}"
echo "${helloCount}"

# Get the number of retries allowed in case of a failing test
retries=$(test-agent-cli retry-count)
echo "no of retries are $retries"

# Get the secret for a key
secret=$(test-agent-cli get-secret key1)
echo "Value for secret is $secret"

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
        # Send the test results for this retry.
        test-agent-cli send-result -o fail -p 0 -f 1 -s 0

        if test; then
            break
        fi
    done
fi

# Send the test results after consuming all the retries
test-agent-cli send-result -o pass -p 1 -f 0 -s 0

touch results.txt
touch results.yaml
mkdir my_results
mkdir finished
# Save the test results as a file or directory
test-agent-cli save-results -f results.txt -f results.yaml -d my_results -d finished

# Mark the test as completed.
test-agent-cli terminate

