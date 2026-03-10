Deploy OpsAgent to AWS from scratch. Follow these steps in order, stopping if any step fails:

## Step 1: Pre-flight Checks

1. Run `aws sts get-caller-identity` to verify AWS credentials
2. Save the account ID and region for later use
3. Check if Bedrock Claude Opus model is accessible: `aws bedrock list-foundation-models --region us-east-1 --query "modelSummaries[?contains(modelId,'claude')].[modelId]" --output text`
4. Check Node.js version: `node --version` (must be >= 20)

Report the results to the user. If credentials are not available, stop and ask the user to fix it.

## Step 2: CDK Bootstrap & Deploy Infrastructure

1. `cd infra && npm install`
2. Check if CDK is bootstrapped: `npx cdk bootstrap aws://$ACCOUNT_ID/$REGION`
3. Deploy the main stack: `npx cdk deploy OpsAgentStack --require-approval never`
   - This creates: VPC, ECS, ALB, EFS, IAM, ECR, CodeBuild, S3 Source Bucket
4. Capture outputs: ALB DNS, EFS ID, Cluster Name, CodeBuild Project, Source Bucket
5. Report all outputs to the user

## Step 3: Configure Platforms

Ask the user which IM platforms they want to enable and collect their credentials:
- **Teams**: App ID, App Password
- **Slack**: Bot Token, Signing Secret
- **Feishu**: App ID, App Secret, Verification Token

Update `config/platforms.yaml` with the credentials and enable the chosen platforms. Example:
```yaml
platforms:
  slack:
    enabled: true
    credentials:
      bot_token: "xoxb-..."
      signing_secret: "..."
    settings:
      allowed_channels: []
```

Alternatively, tell the user they can configure platforms later via Admin UI → Platform Integrations.

## Step 4: Build Docker Image via CodeBuild

No local Docker required. Package source and trigger CodeBuild:

```bash
# Package source (exclude node_modules, .git, cdk.out)
cd /path/to/opsagent  # project root
zip -r /tmp/source.zip . -x "*/node_modules/*" -x "*/.git/*" -x "*/cdk.out/*"

# Upload to S3 source bucket
BUCKET=$(aws cloudformation describe-stacks --stack-name OpsAgentStack --query 'Stacks[0].Outputs[?OutputKey==`SourceBucketName`].OutputValue' --output text --region $REGION)
aws s3 cp /tmp/source.zip s3://$BUCKET/source.zip --region $REGION

# Trigger CodeBuild
BUILD_ID=$(aws codebuild start-build --project-name opsagent-build --region $REGION --query 'build.id' --output text)

# Poll until build completes (check every 20s)
while true; do
  STATUS=$(aws codebuild batch-get-builds --ids "$BUILD_ID" --query 'builds[0].buildStatus' --output text --region $REGION)
  if [ "$STATUS" != "IN_PROGRESS" ]; then break; fi
  sleep 20
done

# Verify build succeeded
if [ "$STATUS" != "SUCCEEDED" ]; then
  echo "CodeBuild failed with status: $STATUS"
  # Show failure details
  aws codebuild batch-get-builds --ids "$BUILD_ID" --query 'builds[0].phases[?phaseStatus==`FAILED`].[phaseType,contexts[0].message]' --output text --region $REGION
  exit 1
fi
```

## Step 5: Start Service & Verify

```bash
SERVICE=$(aws ecs list-services --cluster opsagent-cluster --query 'serviceArns[0]' --output text --region $REGION)
aws ecs update-service --cluster opsagent-cluster --service $SERVICE --force-new-deployment --region $REGION
aws ecs wait services-stable --cluster opsagent-cluster --services $SERVICE --region $REGION
```

Then verify:
```bash
ALB=$(aws cloudformation describe-stacks --stack-name OpsAgentStack --query 'Stacks[0].Outputs[?OutputKey==`AlbDnsName`].OutputValue' --output text --region $REGION)
curl -s http://$ALB/health
```

## Step 6: Report

Provide the user with:
- Health check URL: `http://<ALB>/health`
- Admin UI URL: `http://<ALB>/admin`
- Webhook URLs for each enabled platform: `http://<ALB>/api/messages/{teams,slack,feishu}`
- Remind them to configure webhook URLs in their IM platform developer console
- Remind them they can manage platforms, glossary, skills, and knowledge base via Admin UI

## Optional: Deploy Member Account Roles

Ask the user if they want to deploy cross-account ReadOnly roles to Organization member accounts. If yes:
```bash
cd infra && npx cdk deploy MemberRoleStack -c organizationRootOuId=<ask user> --require-approval never
```
