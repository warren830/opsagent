terraform {
  required_version = ">= 1.0"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.0"
    }
  }
}

data "aws_organizations_organization" "this" {}

locals {
  child_accounts = [
    for acct in data.aws_organizations_organization.this.accounts
    : acct.id if acct.id != var.management_account_id && acct.status == "ACTIVE"
  ]
}

# EKS Pod Identity requires sts:TagSession, but OrganizationAccountAccessRole
# (auto-created by AWS Organizations) only allows sts:AssumeRole by default.
# This also bootstraps an OpsRole (ReadOnlyAccess + targeted ops write) for
# least-privilege access. OrganizationAccountAccessRole is kept as bootstrap/emergency only.
resource "null_resource" "configure_child_account" {
  for_each = toset(local.child_accounts)

  triggers = {
    account_id = each.value
    version    = "v2"
  }

  provisioner "local-exec" {
    interpreter = ["bash", "-euo", "pipefail", "-c"]
    command     = <<-EOT
      ACCT="${each.value}"
      echo "==> Configuring account $ACCT..."

      TRUST_POLICY='{
        "Version": "2012-10-17",
        "Statement": [{
          "Effect": "Allow",
          "Principal": {"AWS": "arn:${var.partition}:iam::${var.management_account_id}:root"},
          "Action": ["sts:AssumeRole", "sts:TagSession"]
        }]
      }'

      OPS_WRITE_POLICY='{
        "Version": "2012-10-17",
        "Statement": [{
          "Sid": "OpsWrite",
          "Effect": "Allow",
          "Action": [
            "ec2:RebootInstances",
            "ec2:StartInstances",
            "ec2:StopInstances",
            "autoscaling:UpdateAutoScalingGroup",
            "autoscaling:SetDesiredCapacity",
            "eks:UpdateNodegroupConfig",
            "ecs:UpdateService",
            "ecs:UpdateCluster",
            "rds:RebootDBInstance",
            "rds:StartDBInstance",
            "rds:StopDBInstance",
            "lambda:UpdateFunctionConfiguration",
            "lambda:InvokeFunction"
          ],
          "Resource": "*"
        }]
      }'

      CREDS=$(aws sts assume-role \
        --role-arn "arn:${var.partition}:iam::$ACCT:role/OrganizationAccountAccessRole" \
        --role-session-name "tf-cross-account-setup" \
        --query 'Credentials.[AccessKeyId,SecretAccessKey,SessionToken]' \
        --output text)

      export AWS_ACCESS_KEY_ID=$(echo "$CREDS" | awk '{print $1}')
      export AWS_SECRET_ACCESS_KEY=$(echo "$CREDS" | awk '{print $2}')
      export AWS_SESSION_TOKEN=$(echo "$CREDS" | awk '{print $3}')

      aws iam update-assume-role-policy \
        --role-name OrganizationAccountAccessRole \
        --policy-document "$TRUST_POLICY"
      echo "  OK: OrganizationAccountAccessRole trust updated"

      # Create OpsRole or update its trust policy (idempotent)
      if aws iam create-role \
           --role-name OpsRole \
           --assume-role-policy-document "$TRUST_POLICY" \
           --tags Key=ManagedBy,Value=terraform Key=Project,Value=ops${join("", [for k, v in var.default_tags : " Key=${k},Value=${v}"])} \
           2>/dev/null; then
        echo "  OK: OpsRole created"
      else
        aws iam update-assume-role-policy \
          --role-name OpsRole \
          --policy-document "$TRUST_POLICY"
        echo "  OK: OpsRole trust updated"
      fi

      # Attach managed ReadOnlyAccess
      aws iam attach-role-policy \
        --role-name OpsRole \
        --policy-arn "arn:${var.partition}:iam::aws:policy/ReadOnlyAccess" \
        2>/dev/null || true

      # Attach inline ops-write policy (idempotent put)
      aws iam put-role-policy \
        --role-name OpsRole \
        --policy-name OpsWrite \
        --policy-document "$OPS_WRITE_POLICY"
      echo "  OK: OpsWrite inline policy attached"

      echo "==> Account $ACCT done"
    EOT
  }
}
