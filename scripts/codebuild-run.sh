#!/bin/bash
#
# Run the Ops Docker build on AWS CodeBuild (arm64 Graviton worker).
#
# Idempotent — creates the CodeBuild project + IAM role + source S3 bucket
# on first run, reuses them on subsequent runs. Packages the repo, uploads
# to S3, starts the build, and streams logs until completion.
#
# Usage: ./scripts/codebuild-run.sh
#
set -euo pipefail

export AWS_PAGER=""
# Bypass macOS / shell HTTP proxies for AWS endpoints. boto3 honours NO_PROXY
# with domain-suffix matching, which covers s3.*.amazonaws.com,
# codebuild.*.amazonaws.com, etc. Without this, upload hangs on slow or
# stale local proxies (e.g. the 127.0.0.1:7890 Clash/VPN socket).
export NO_PROXY="amazonaws.com,amazonaws.com.cn,localhost,127.0.0.1"
export no_proxy="$NO_PROXY"
unset HTTP_PROXY HTTPS_PROXY http_proxy https_proxy 2>/dev/null || true

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
IAC_DIR="$PROJECT_ROOT/iac"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'
log()  { echo -e "${GREEN}[codebuild]${NC} $*"; }
warn() { echo -e "${YELLOW}[codebuild]${NC} $*"; }
err()  { echo -e "${RED}[codebuild]${NC} $*" >&2; }

# ── Config (derived) ────────────────────────────────────────────
REGION=$(cd "$IAC_DIR" && terraform output -raw region 2>/dev/null || echo "${AWS_REGION:-us-west-2}")
ACCOUNT=$(aws sts get-caller-identity --query Account --output text)
PROJECT_NAME="ops-build"
ROLE_NAME="ops-codebuild-role"
SOURCE_BUCKET="ops-codebuild-source-${ACCOUNT}-${REGION}"
SOURCE_KEY="ops-source.zip"

log "Region      = $REGION"
log "Account     = $ACCOUNT"
log "Project     = $PROJECT_NAME"
log "Role        = $ROLE_NAME"
log "Source S3   = s3://$SOURCE_BUCKET/$SOURCE_KEY"

# ── 1. Source bucket ────────────────────────────────────────────
if ! aws s3api head-bucket --bucket "$SOURCE_BUCKET" --region "$REGION" 2>/dev/null; then
    log "Creating source bucket $SOURCE_BUCKET"
    if [[ "$REGION" == "us-east-1" ]]; then
        aws s3api create-bucket --bucket "$SOURCE_BUCKET" --region "$REGION" >/dev/null
    else
        aws s3api create-bucket --bucket "$SOURCE_BUCKET" --region "$REGION" \
            --create-bucket-configuration LocationConstraint="$REGION" >/dev/null
    fi
    aws s3api put-bucket-versioning --bucket "$SOURCE_BUCKET" \
        --versioning-configuration Status=Enabled >/dev/null
    aws s3api put-public-access-block --bucket "$SOURCE_BUCKET" \
        --public-access-block-configuration \
        'BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true' >/dev/null
fi

# ── 2. IAM role ─────────────────────────────────────────────────
TRUST_POLICY='{
  "Version":"2012-10-17",
  "Statement":[{"Effect":"Allow","Principal":{"Service":"codebuild.amazonaws.com"},"Action":"sts:AssumeRole"}]
}'

ROLE_POLICY=$(cat <<EOF
{
  "Version":"2012-10-17",
  "Statement":[
    {"Effect":"Allow","Action":["logs:CreateLogGroup","logs:CreateLogStream","logs:PutLogEvents"],"Resource":"arn:aws:logs:$REGION:$ACCOUNT:log-group:/aws/codebuild/$PROJECT_NAME:*"},
    {"Effect":"Allow","Action":["s3:GetObject","s3:GetObjectVersion"],"Resource":"arn:aws:s3:::$SOURCE_BUCKET/*"},
    {"Effect":"Allow","Action":["s3:ListBucket"],"Resource":"arn:aws:s3:::$SOURCE_BUCKET"},
    {"Effect":"Allow","Action":["ecr:GetAuthorizationToken"],"Resource":"*"},
    {"Effect":"Allow","Action":["ecr:BatchCheckLayerAvailability","ecr:CompleteLayerUpload","ecr:CreateRepository","ecr:DescribeRepositories","ecr:InitiateLayerUpload","ecr:PutImage","ecr:UploadLayerPart","ecr:BatchGetImage","ecr:GetDownloadUrlForLayer","ecr:TagResource"],"Resource":"*"}
  ]
}
EOF
)

if ! aws iam get-role --role-name "$ROLE_NAME" >/dev/null 2>&1; then
    log "Creating IAM role $ROLE_NAME"
    aws iam create-role --role-name "$ROLE_NAME" \
        --assume-role-policy-document "$TRUST_POLICY" >/dev/null
    aws iam put-role-policy --role-name "$ROLE_NAME" \
        --policy-name ops-codebuild-inline \
        --policy-document "$ROLE_POLICY" >/dev/null
    log "Waiting 10s for IAM propagation..."
    sleep 10
else
    # Refresh inline policy (idempotent)
    aws iam put-role-policy --role-name "$ROLE_NAME" \
        --policy-name ops-codebuild-inline \
        --policy-document "$ROLE_POLICY" >/dev/null
fi
ROLE_ARN="arn:aws:iam::$ACCOUNT:role/$ROLE_NAME"

# ── 3. CodeBuild project ────────────────────────────────────────
PROJECT_JSON=$(cat <<EOF
{
  "name": "$PROJECT_NAME",
  "description": "Ops backend+frontend build, ARM64 -> ECR",
  "source": {
    "type": "S3",
    "location": "$SOURCE_BUCKET/$SOURCE_KEY"
  },
  "artifacts": {"type": "NO_ARTIFACTS"},
  "environment": {
    "type": "ARM_CONTAINER",
    "image": "aws/codebuild/amazonlinux-aarch64-standard:3.0",
    "computeType": "BUILD_GENERAL1_LARGE",
    "privilegedMode": true,
    "environmentVariables": [
      {"name":"AWS_ACCOUNT_ID","value":"$ACCOUNT"},
      {"name":"AWS_REGION","value":"$REGION"}
    ]
  },
  "serviceRole": "$ROLE_ARN",
  "timeoutInMinutes": 60,
  "logsConfig": {"cloudWatchLogs": {"status": "ENABLED"}}
}
EOF
)

PROJECT_JSON_FILE=$(mktemp -t ops-cb-project.XXXXXX.json)
printf '%s' "$PROJECT_JSON" > "$PROJECT_JSON_FILE"
if aws codebuild batch-get-projects --names "$PROJECT_NAME" --region "$REGION" \
    --query 'projects[0].name' --output text 2>/dev/null | grep -q "$PROJECT_NAME"; then
    log "Updating CodeBuild project"
    aws codebuild update-project --cli-input-json "file://$PROJECT_JSON_FILE" \
        --region "$REGION" >/dev/null
else
    log "Creating CodeBuild project"
    aws codebuild create-project --cli-input-json "file://$PROJECT_JSON_FILE" \
        --region "$REGION" >/dev/null
fi
rm -f "$PROJECT_JSON_FILE"

# ── 4. Package source and upload ────────────────────────────────
log "Packaging source (excluding build caches)..."
cd "$PROJECT_ROOT"
TMP_ZIP="$(mktemp -d -t ops-source.XXXXXX)/ops-source.zip"
# -x for excludes. Keep it tight to avoid huge uploads.
zip -rq "$TMP_ZIP" . \
    -x '.git/*' 'node_modules/*' '*/node_modules/*' \
       'backend/target/*' 'frontend/.nuxt/*' 'frontend/.output/*' \
       'e2e/node_modules/*' 'e2e/report/*' 'e2e/test-results/*' 'e2e/screenshots/*' \
       'iac/.terraform/*' '.playwright-mcp/*' \
       'demo/frames/*' 'demo/video/*' 'demo/narration/*'
log "Uploading source to s3://$SOURCE_BUCKET/$SOURCE_KEY"
aws s3 cp "$TMP_ZIP" "s3://$SOURCE_BUCKET/$SOURCE_KEY" --region "$REGION" >/dev/null
rm -rf "$(dirname "$TMP_ZIP")"

# ── 5. Start build and stream logs ──────────────────────────────
log "Starting CodeBuild..."
BUILD_ID=$(aws codebuild start-build --project-name "$PROJECT_NAME" --region "$REGION" \
    --query 'build.id' --output text)
log "Build ID: $BUILD_ID"
log "Console: https://$REGION.console.aws.amazon.com/codesuite/codebuild/projects/$PROJECT_NAME/build/$BUILD_ID"

# Poll for completion
while true; do
    STATUS=$(aws codebuild batch-get-builds --ids "$BUILD_ID" --region "$REGION" \
        --query 'builds[0].buildStatus' --output text)
    case "$STATUS" in
        IN_PROGRESS)
            CURRENT_PHASE=$(aws codebuild batch-get-builds --ids "$BUILD_ID" --region "$REGION" \
                --query 'builds[0].currentPhase' --output text)
            log "  status=IN_PROGRESS phase=$CURRENT_PHASE"
            sleep 20
            ;;
        SUCCEEDED)
            log "Build SUCCEEDED"
            break
            ;;
        *)
            err "Build $STATUS — see CloudWatch logs at https://$REGION.console.aws.amazon.com/codesuite/codebuild/projects/$PROJECT_NAME/build/$BUILD_ID"
            exit 1
            ;;
    esac
done

log "Images pushed to ECR in $REGION"
