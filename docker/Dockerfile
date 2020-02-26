FROM gitpod/workspace-full:latest

# Install libfuse-dev
USER root
RUN apt-get update && apt-get install -y fdupes linux-tools-common linux-tools-generic \
    && apt-get clean && rm -rf /var/cache/apt/* && rm -rf /var/lib/apt/lists/* && rm -rf /tmp/*
