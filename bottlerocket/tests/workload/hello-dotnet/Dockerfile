FROM public.ecr.aws/amazonlinux/amazonlinux:2023

# Install necessary utilities
RUN dnf makecache && \
    dnf install -y \
      gzip \
      tar \
      dotnet \
    && \
    dnf clean all && \
    rm -rf /var/cache/dnf

# Disable NuGet network access
ENV DOTNET_CLI_TELEMETRY_OPTOUT=1
ENV DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1
ENV NUGET_XMLDOC_MODE=skip

# Copy the script that will run the application
COPY --chmod=755 ./run.sh /
ENTRYPOINT ["./run.sh"]

