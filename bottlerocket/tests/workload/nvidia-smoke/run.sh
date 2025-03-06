#! /usr/bin/env bash

set -e
set -o pipefail

# Collect the test output where sonobuoy expects plugins to place them
results_dir="${RESULTS_DIR:-/tmp/results}"
results_tar="results.tar.gz"
mkdir -p "${results_dir}"

testDone() {
    echo "${results_dir}/${results_tar}" >"${results_dir}/done"
}

# Make sure to always output done file in expected place and format
trap testDone EXIT

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

# Collect the results
tar czf "${results_tar}" -C "${results_dir}" .
mv "${results_tar}" "${results_dir}/"
