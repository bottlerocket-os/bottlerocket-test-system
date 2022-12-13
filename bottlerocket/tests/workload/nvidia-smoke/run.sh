#! /usr/bin/env bash

set -e
set -o pipefail

# Collect the test output where sonobuoy expects plugins to place them
results_dir="${RESULTS_DIR:-/tmp/results}"
mkdir -p "${results_dir}"

saveResults() {
     cd "${results_dir}"
     tar czf results.tar.gz ./*
     echo "${results_dir}/results.tar.gz" > "${results_dir}/done"
}

# Make sure to always capture results in expected place and format
trap saveResults EXIT

# Run the CUDA sample binaries to exercise various GPU functions
cd /samples
for sample in *; do
    echo
    echo "========================================="
    echo "  Running sample ${sample}"
    echo "========================================="
    echo
    "./${sample}" 2>&1 | tee "${results_dir}/${sample}.log"
done
