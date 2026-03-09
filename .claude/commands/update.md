Update the deployed OpsAgent service with the latest code changes.

## Steps

1. **Detect what changed** - Run `git diff --stat HEAD` and `git status` to understand what files changed.

2. **Determine update scope**:
   - If only `config/` files changed → config-only update (rebuild image + redeploy)
   - If `infra/` files changed → CDK deploy first, then rebuild + redeploy
   - If `bot/` or `Dockerfile` changed → rebuild image + redeploy
   - If only `knowledge/` changed → no rebuild needed (EFS is persistent), but files baked in Dockerfile need rebuild

3. **CDK deploy** (if infra changed):
   ```bash
   cd infra && npx cdk deploy OpsAgentStack --require-approval never
   ```

4. **Build via CodeBuild** (no local Docker required):
   ```bash
   ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
   REGION=us-east-1

   # Package and upload source
   cd /path/to/opsagent
   zip -r /tmp/source.zip . -x "*/node_modules/*" -x "*/.git/*" -x "*/cdk.out/*"
   BUCKET=$(aws cloudformation describe-stacks --stack-name OpsAgentStack --query 'Stacks[0].Outputs[?OutputKey==`SourceBucketName`].OutputValue' --output text --region $REGION)
   aws s3 cp /tmp/source.zip s3://$BUCKET/source.zip --region $REGION

   # Trigger CodeBuild and wait
   BUILD_ID=$(aws codebuild start-build --project-name opsagent-build --region $REGION --query 'build.id' --output text)
   while true; do
     STATUS=$(aws codebuild batch-get-builds --ids "$BUILD_ID" --query 'builds[0].buildStatus' --output text --region $REGION)
     if [ "$STATUS" != "IN_PROGRESS" ]; then break; fi
     sleep 20
   done
   echo "CodeBuild: $STATUS"
   ```

5. **Rolling update**:
   ```bash
   SERVICE=$(aws ecs list-services --cluster opsagent-cluster --query 'serviceArns[0]' --output text --region $REGION)
   aws ecs update-service --cluster opsagent-cluster --service $SERVICE --force-new-deployment --region $REGION
   aws ecs wait services-stable --cluster opsagent-cluster --services $SERVICE --region $REGION
   ```

6. **Verify**:
   ```bash
   ALB=$(aws cloudformation describe-stacks --stack-name OpsAgentStack --region $REGION --query 'Stacks[0].Outputs[?OutputKey==`AlbDnsName`].OutputValue' --output text)
   curl -s http://$ALB/health
   ```

Report the update result to the user.
