FROM public.ecr.aws/amazonlinux/amazonlinux:2023

RUN dnf update && \
    dnf install -y gzip tar && \
    dnf clean all

RUN cat > run.sh <<EOF
#!/usr/bin/env bash
set -e

# Collect the test output where sonobuoy expects plugins to place them
results_dir="\${RESULTS_DIR:-/tmp/results}"
mkdir -p "\${results_dir}"

# Hello!
echo "hello, testsys" | tee "\${results_dir}/hello-testsys.log"

# Save results
cd "\${results_dir}"
tar czf results.tar.gz ./*
echo "\${results_dir}/results.tar.gz" > "\${results_dir}/done"
EOF

RUN chmod +x ./run.sh

ENTRYPOINT ["./run.sh"]
