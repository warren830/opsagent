Help the user add an external AWS account (not in the Organization) to OpsAgent.

Ask the user for:
1. Target account ID
2. Account name/alias
3. Regions to query (default: us-east-1)

## Step 1: Create IAM Role in Target Account

Generate and show the user the trust policy and commands they need to run in the TARGET account:

```bash
# Run this in the TARGET account (switch credentials first)
aws iam create-role \
  --role-name OpsAgentReadOnly \
  --assume-role-policy-document '{
    "Version": "2012-10-17",
    "Statement": [{
      "Effect": "Allow",
      "Principal": {"AWS": "arn:aws:iam::<HUB_ACCOUNT_ID>:role/OpsAgentTaskRole"},
      "Action": "sts:AssumeRole"
    }]
  }'

aws iam attach-role-policy \
  --role-name OpsAgentReadOnly \
  --policy-arn arn:aws:iam::aws:policy/ReadOnlyAccess
```

If the user wants a custom role name, adjust accordingly.

## Step 2: Add to OpsAgent Config

Update `config/accounts.yaml` to add the account under `extra`:

```yaml
accounts:
  extra:
    - id: "<account_id>"
      name: "<account_name>"
      role_name: "<role_name>"  # if different from default
      regions: ["<region>"]
```

Or use the Admin API:
```bash
ALB=$(aws cloudformation describe-stacks --stack-name OpsAgentStack --region us-east-1 --query 'Stacks[0].Outputs[?OutputKey==`AlbDnsName`].OutputValue' --output text)
# GET current config, add account, PUT back
```

## Step 3: Verify Connectivity

Test the AssumeRole from the hub account:
```bash
aws sts assume-role \
  --role-arn arn:aws:iam::<TARGET_ACCOUNT>:role/<ROLE_NAME> \
  --role-session-name test
```

If successful, report to the user. If not, help debug (common issues: trust policy, role name mismatch).

## Step 4: Rebuild & Deploy (if config file changed)

If `config/accounts.yaml` was modified directly (not via Admin API), rebuild and redeploy:
```bash
docker build --platform linux/amd64 -t opsagent:latest .
# ... tag, push, force-new-deployment
```

If modified via Admin API, the change takes effect on the next query (no rebuild needed, but note the config is in-container and will be lost on next redeploy unless also updated in the source file).
