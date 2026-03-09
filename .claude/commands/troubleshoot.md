Troubleshoot OpsAgent deployment issues. Run diagnostics and identify the problem.

## Diagnostic Steps

Run these checks in parallel where possible:

### 1. Stack Status
```bash
aws cloudformation describe-stacks --stack-name OpsAgentStack --region us-east-1 --query 'Stacks[0].StackStatus' --output text
```
If not `*_COMPLETE`, check events:
```bash
aws cloudformation describe-stack-events --stack-name OpsAgentStack --region us-east-1 --query 'StackEvents[?contains(ResourceStatus,`FAILED`)].[LogicalResourceId,ResourceStatusReason]' --output table
```

### 2. ECS Service
```bash
SERVICE=$(aws ecs list-services --cluster opsagent-cluster --query 'serviceArns[0]' --output text --region us-east-1)
aws ecs describe-services --cluster opsagent-cluster --services $SERVICE --region us-east-1 --query 'services[0].{status:status,desired:desiredCount,running:runningCount,events:events[0:5]}'
```

### 3. Task Status
```bash
# Running tasks
aws ecs list-tasks --cluster opsagent-cluster --region us-east-1

# Stopped tasks (recent failures)
aws ecs list-tasks --cluster opsagent-cluster --desired-status STOPPED --region us-east-1
```
For stopped tasks, describe them to get stop reason:
```bash
aws ecs describe-tasks --cluster opsagent-cluster --tasks <TASK_ARN> --region us-east-1 --query 'tasks[0].{stopCode:stopCode,stopReason:stoppedReason,containers:containers[*].{name:name,exitCode:exitCode,reason:reason}}'
```

### 4. Logs
```bash
aws logs tail /ecs/opsagent --since 15m --region us-east-1 | tail -50
```
Look for:
- `Error` or `error` messages
- `Claude Code exited with code` (Claude CLI failures)
- `ECONNREFUSED` (network issues)
- `UnauthorizedException` (Bedrock permission issues)

### 5. ALB Target Health
```bash
TG_ARN=$(aws elbv2 describe-target-groups --names $(aws elbv2 describe-listeners --load-balancer-arn $(aws elbv2 describe-load-balancers --names opsagent-alb --region us-east-1 --query 'LoadBalancers[0].LoadBalancerArn' --output text) --region us-east-1 --query 'Listeners[0].DefaultActions[0].TargetGroupArn' --output text) --region us-east-1 --query 'TargetGroups[0].TargetGroupArn' --output text 2>/dev/null)
aws elbv2 describe-target-health --target-group-arn $TG_ARN --region us-east-1 2>/dev/null
```

### 6. EFS Mount
```bash
EFS_ID=$(aws cloudformation describe-stacks --stack-name OpsAgentStack --region us-east-1 --query 'Stacks[0].Outputs[?OutputKey==`EfsFileSystemId`].OutputValue' --output text)
aws efs describe-mount-targets --file-system-id $EFS_ID --region us-east-1 --query 'MountTargets[*].{AZ:AvailabilityZoneName,State:LifeCycleState,SubnetId:SubnetId}'
```

### 7. Security Groups
Check if ECS tasks can reach EFS (port 2049) and ALB can reach ECS (port 3978).

## Common Issues & Fixes

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| Task keeps restarting | Health check failing, OOM, or crash | Check logs, increase memory |
| 502 from ALB | Task not healthy yet or crashed | Wait or check task logs |
| Bedrock errors in logs | Model not enabled or IAM issue | Enable model in Bedrock console, check TaskRole |
| EFS mount timeout | Security group or mount target issue | Check SG allows 2049 from ECS |
| Claude Code timeout | Query too complex | Increase CLAUDE_TIMEOUT_MS or simplify query |
| "not a git repository" | Dockerfile git init issue | Rebuild image |

Analyze the diagnostics results and provide a clear diagnosis with recommended fix.
