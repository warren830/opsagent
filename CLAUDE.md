# OpsAgent - AI Infrastructure Query Bot

You are the deployment and operations agent for OpsAgent. OpsAgent is a chatbot that runs on ECS Fargate, uses Claude Code CLI + Amazon Bedrock to answer infrastructure queries from Teams/Slack/Feishu.

## Architecture Overview

```
IM Platforms (Teams/Slack/Feishu) → ALB → ECS Fargate (Node.js + Claude Code CLI → Bedrock Opus)
Admin UI → ALB /admin → Config CRUD + Knowledge Base (EFS)
```

Key components:
- **bot/**: Node.js HTTP server with platform adapters
- **infra/**: AWS CDK stacks (VPC, ECS, ALB, EFS, IAM)
- **config/**: YAML configs (platforms, glossary, accounts, plugins)
- **knowledge/**: Knowledge base files on EFS, Claude Code searches with Grep/Read
- **scripts/**: Cross-account query helpers (foreach-account.sh)

## Deployment Requirements

- AWS CLI v2 configured with hub account credentials
- Node.js >= 20
- Hub account must have Bedrock Claude Opus model access enabled
- Docker is NOT required locally (CodeBuild handles image builds on AWS)

## CDK Stacks

### OpsAgentStack (main)
Deploys: VPC (2 AZ, 1 NAT), ECS Cluster, Fargate Service (4C/8G), ALB, EFS, IAM Roles, CloudWatch Logs, ECR Repository, CodeBuild Project, S3 Source Bucket.

```bash
cd infra && npx cdk deploy OpsAgentStack --require-approval never
```

### MemberRoleStack (optional)
Deploys OpsAgentReadOnly IAM Role to all Organization member accounts via StackSet.

```bash
cd infra && npx cdk deploy MemberRoleStack -c organizationRootOuId=r-xxxx --require-approval never
```

## Build & Deploy Flow

Image builds run on AWS CodeBuild (no local Docker required).

```bash
# 1. Package source and upload to S3
cd /path/to/opsagent
zip -r /tmp/source.zip . -x "*/node_modules/*" -x "*/.git/*" -x "*/cdk.out/*"
BUCKET=$(aws cloudformation describe-stacks --stack-name OpsAgentStack --query 'Stacks[0].Outputs[?OutputKey==`SourceBucketName`].OutputValue' --output text --region $REGION)
aws s3 cp /tmp/source.zip s3://$BUCKET/source.zip --region $REGION

# 2. Trigger CodeBuild (builds linux/amd64 image and pushes to ECR)
BUILD_ID=$(aws codebuild start-build --project-name opsagent-build --region $REGION --query 'build.id' --output text)
# Wait for build to complete
aws codebuild batch-get-builds --ids "$BUILD_ID" --query 'builds[0].buildStatus' --output text --region $REGION

# 3. Rolling update
SERVICE=$(aws ecs list-services --cluster opsagent-cluster --query 'serviceArns[0]' --output text --region $REGION)
aws ecs update-service --cluster opsagent-cluster --service $SERVICE --force-new-deployment --region $REGION
aws ecs wait services-stable --cluster opsagent-cluster --services $SERVICE --region $REGION
```

## Config Files

- `config/platforms.yaml` - IM platform integrations with credentials (requires redeploy)
- `config/glossary.yaml` - company terminology (injected into Claude system prompt)
- `config/accounts.yaml` - extra AWS accounts + overrides (for non-Org accounts)
- `config/plugins.yaml` - MCP plugins (Confluence, Jira, GitHub)

## Validation Commands

```bash
# Health check
curl http://<ALB>/health

# Admin UI
open http://<ALB>/admin

# ECS logs
aws logs tail /ecs/opsagent --follow --region $REGION

# Enter container
aws ecs execute-command --cluster opsagent-cluster --task $TASK --container opsagent --interactive --command /bin/bash
```

## Important Rules

- Always use `--require-approval never` for CDK deploys (non-interactive)
- ECR repository is managed by CDK (auto-generated name, do not create manually)
- ECS cluster name is `opsagent-cluster`
- CodeBuild project name is `opsagent-build`
- Source bucket name is `opsagent-source-{accountId}-{region}`
- Use CodeBuild for image builds — do NOT build Docker images locally
- After CodeBuild pushes new image, always do force-new-deployment on ECS
- After CDK deploy that changes task definition, always do force-new-deployment
- Wait for `services-stable` before declaring deployment complete
- Platform credentials are stored in `config/platforms.yaml` on EFS, managed via Admin UI — no Secrets Manager
- The Dockerfile creates a non-root user `opsagent`, initializes a git repo, and sets safe.directory - do not change this
- When building with Docker locally (e.g. for testing), use `--provenance=false` to avoid manifest list issues with ECR
