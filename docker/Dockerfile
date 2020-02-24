FROM gitpod/workspace-full:latest

# Install libfuse-dev
USER root
RUN apt-get update && apt-get install -y fdupes \
    && apt-get clean && rm -rf /var/cache/apt/* && rm -rf /var/lib/apt/lists/* && rm -rf /tmp/*
