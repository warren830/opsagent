Destroy the OpsAgent deployment. THIS IS A DESTRUCTIVE OPERATION.

**Always ask the user to confirm before proceeding. List exactly what will be deleted.**

## What will be destroyed

- ECS Service and Task (stops the bot)
- ALB (webhook URLs will stop working)
- VPC, Subnets, NAT Gateway
- CloudWatch Log Group
- IAM Roles (TaskRole, ExecutionRole)
- ECR repository and all images (managed by CDK)
- CodeBuild project
- S3 source bucket
- EFS filesystem is set to RETAIN and will NOT be deleted (knowledge base preserved)

## Steps

1. **Confirm with user**: "This will destroy the OpsAgent stack. The EFS filesystem will be retained. Type 'yes' to proceed."

2. **Destroy the stack**:
   ```bash
   cd infra && npx cdk destroy OpsAgentStack --force
   ```

3. **(Optional) Destroy member role StackSet**:
   ```bash
   npx cdk destroy MemberRoleStack --force
   ```

4. **(Optional) Clean up retained EFS**:
   - EFS: `aws efs delete-file-system --file-system-id <id> --region us-east-1` (must delete mount targets first)

Report what was destroyed and what was retained.
