FROM public.ecr.aws/amazonlinux/amazonlinux:2023

RUN dnf update && \
    dnf install -y gzip tar && \
    dnf clean all

RUN cat > run.sh <<EOF
#!/usr/bin/env bash
set -e

RUN chmod +x ./run.sh

ENTRYPOINT ["./run.sh"]
