Check the current deployment status of OpsAgent. Run all checks and report a summary.

## Checks to Run

1. **AWS Identity**
   ```bash
   aws sts get-caller-identity
   ```

2. **CloudFormation Stack**
   ```bash
   aws cloudformation describe-stacks --stack-name OpsAgentStack --region us-east-1 --query 'Stacks[0].{Status:StackStatus,Outputs:Outputs[*].{Key:OutputKey,Value:OutputValue}}' 2>/dev/null
   ```

3. **ECS Service**
   ```bash
   aws ecs describe-services --cluster opsagent-cluster --services $(aws ecs list-services --cluster opsagent-cluster --query 'serviceArns[0]' --output text --region us-east-1) --region us-east-1 --query 'services[0].{desired:desiredCount,running:runningCount,pending:pendingCount,status:status,deployments:deployments[*].{status:status,desired:desiredCount,running:runningCount,rollout:rolloutState}}'
   ```

4. **Task Health**
   ```bash
   TASK=$(aws ecs list-tasks --cluster opsagent-cluster --query 'taskArns[0]' --output text --region us-east-1)
   aws ecs describe-tasks --cluster opsagent-cluster --tasks $TASK --region us-east-1 --query 'tasks[0].{status:lastStatus,health:healthStatus,startedAt:startedAt,cpu:cpu,memory:memory}'
   ```

5. **ALB Health**
   ```bash
   ALB=$(aws cloudformation describe-stacks --stack-name OpsAgentStack --region us-east-1 --query 'Stacks[0].Outputs[?OutputKey==`AlbDnsName`].OutputValue' --output text)
   curl -s http://$ALB/health
   ```

6. **Recent Logs** (last 20 lines)
   ```bash
   aws logs tail /ecs/opsagent --since 5m --region us-east-1 | tail -20
   ```

7. **EFS Status**
   ```bash
   EFS_ID=$(aws cloudformation describe-stacks --stack-name OpsAgentStack --region us-east-1 --query 'Stacks[0].Outputs[?OutputKey==`EfsFileSystemId`].OutputValue' --output text)
   aws efs describe-file-systems --file-system-id $EFS_ID --region us-east-1 --query 'FileSystems[0].{State:LifeCycleState,Size:SizeInBytes.Value}'
   ```

8. **ECR Image**
   ```bash
   aws ecr describe-images --repository-name opsagent --region us-east-1 --query 'imageDetails | sort_by(@, &imagePushedAt) | [-1].{pushed:imagePushedAt,size:imageSizeInBytes,tags:imageTags}'
   ```

## Output Format

Present results as a summary table:

| Component | Status | Details |
|-----------|--------|---------|
| Stack | ... | ... |
| Service | ... | running/desired |
| Task | ... | health, uptime |
| ALB | ... | health response |
| EFS | ... | state, size |
| Image | ... | last pushed |
| Logs | ... | any errors? |
