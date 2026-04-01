FROM public.ecr.aws/docker/library/node:20-slim

RUN apt-get update && apt-get install -y curl unzip jq git dnsutils && rm -rf /var/lib/apt/lists/*

# AWS CLI
RUN curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip" \
    && unzip awscliv2.zip && ./aws/install && rm -rf awscliv2.zip aws

# kubectl
RUN curl -LO "https://dl.k8s.io/release/$(curl -L -s https://dl.k8s.io/release/stable.txt)/bin/linux/amd64/kubectl" \
    && install kubectl /usr/local/bin/kubectl && rm kubectl

# Alibaba Cloud CLI
RUN curl -Lo aliyun-cli.tgz https://aliyuncli.alicdn.com/aliyun-cli-linux-latest-amd64.tgz \
    && tar xzf aliyun-cli.tgz && mv aliyun /usr/local/bin/ && rm -f aliyun-cli.tgz

# Claude Code CLI (uses Bedrock as backend)
RUN npm install -g @anthropic-ai/claude-code

WORKDIR /app

# Copy project files
COPY config/ ./config/
COPY scripts/ ./scripts/
COPY knowledge/ ./knowledge/
# Keep a copy of static knowledge files — EFS mount will overlay knowledge/
RUN cp -r ./knowledge/ ./knowledge-defaults/

# Install all deps (including devDependencies for tsc), build, then prune
COPY bot/package.json bot/package-lock.json* ./bot/
RUN cd bot && npm install
COPY bot/ ./bot/
RUN cd bot && npm run build && npm prune --omit=dev

# Initialize git repo (Claude Code requires a git repository)
RUN cd /app && git init && git add -A && git -c user.email="ops@agent" -c user.name="OpsAgent" commit -m "init" --allow-empty

# Run as non-root user
RUN useradd -m -s /bin/bash opsagent && chown -R opsagent:opsagent /app && \
    mkdir -p /home/opsagent/.claude && \
    echo '{"hasCompletedOnboarding":true,"hasAcknowledgedTerms":true}' > /home/opsagent/.claude/settings.json && \
    chown -R opsagent:opsagent /home/opsagent/.claude && \
    rm -f /home/opsagent/.gitconfig && \
    git config --system --add safe.directory /app
USER opsagent

EXPOSE 3978

HEALTHCHECK --interval=30s --timeout=3s CMD curl -f http://localhost:3978/health || exit 1

# On startup: sync static knowledge files to EFS (skip existing to preserve user edits)
CMD cp -rn ./knowledge-defaults/. ./knowledge/ 2>/dev/null; node bot/dist/index.js
