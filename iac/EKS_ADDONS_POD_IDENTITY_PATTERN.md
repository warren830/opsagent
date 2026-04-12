# EKS Add-ons Pod Identity Pattern - Complete Reference

This document provides a detailed reference for the Pod Identity pattern used in the OpenOps infrastructure, specifically for the EKS Add-ons module.

## Overview

The `eks-addons` module implements the **EKS Pod Identity** (not IRSA - IAM Roles for Service Accounts) pattern to grant AWS permissions to Kubernetes workloads running in the cluster.

**Key Difference:** Pod Identity is newer, simpler, and doesn't require webhooks. It authenticates pods directly via `pods.eks.amazonaws.com` principal.

---

## Pattern Structure

### Step 1: Create IAM Policy Document (Trust Policy)

```hcl
data "aws_iam_policy_document" "service_name" {
  statement {
    effect = "Allow"
    
    principals {
      type        = "Service"
      identifiers = ["pods.eks.amazonaws.com"]  # EKS Pod Identity principal
    }
    
    actions = [
      "sts:AssumeRole",
      "sts:TagSession"
    ]
  }
}
```

**Key Points:**
- Principal is always `pods.eks.amazonaws.com` (not an OIDC provider like IRSA)
- Actions are always `sts:AssumeRole` and `sts:TagSession`

---

### Step 2: Create IAM Role

```hcl
resource "aws_iam_role" "service_name" {
  name               = "${local.resource_prefix}-service-name"
  assume_role_policy = data.aws_iam_policy_document.service_name.json
  tags               = var.default_tags
}
```

**Naming Convention:** `${resource_prefix}-{service-short-name}`

Examples:
- `oops-612674025488-us-west-1-dev-albc` (ALB Controller)
- `oops-612674025488-us-west-1-dev-backend` (Backend app)
- `oops-612674025488-us-west-1-dev-eso` (External Secrets)

---

### Step 3: Create IAM Policy (Permissions)

Two options:

#### Option A: Reference AWS Managed Policy
```hcl
resource "aws_iam_policy" "service_name" {
  policy = templatefile("${path.module}/policies/ServicePolicy.json", {
    partition = var.partition
  })
  name = "${local.resource_prefix}-service-name"
  tags = var.default_tags
}
```

#### Option B: Inline Policy
```hcl
resource "aws_iam_policy" "service_name" {
  name = "${local.resource_prefix}-service-name"
  
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "service:Action1",
          "service:Action2"
        ]
        Resource = "arn:aws:service:region:account:resource"
      }
    ]
  })
  
  tags = var.default_tags
}
```

---

### Step 4: Attach Policy to Role

```hcl
resource "aws_iam_role_policy_attachment" "service_name" {
  policy_arn = aws_iam_policy.service_name.arn
  role       = aws_iam_role.service_name.name
}
```

---

### Step 5: Create Pod Identity Association

```hcl
resource "aws_eks_pod_identity_association" "service_name" {
  cluster_name    = var.cluster_name
  namespace       = "kubernetes-namespace"
  service_account = "service-account-name"
  role_arn        = aws_iam_role.service_name.arn
}
```

**Key Fields:**
- `cluster_name`: EKS cluster where pods run
- `namespace`: Kubernetes namespace
- `service_account`: Kubernetes service account name
- `role_arn`: The IAM role ARN created in Step 2

---

## Real-World Examples from eks-addons

### Example 1: AWS Load Balancer Controller

**Purpose:** Manage ALB/NLB creation and configuration

**Code:**
```hcl
# Step 1 & 2: Trust policy + IAM role
data "aws_iam_policy_document" "aws_lbc" {
  statement {
    effect = "Allow"
    principals {
      type        = "Service"
      identifiers = ["pods.eks.amazonaws.com"]
    }
    actions = ["sts:AssumeRole", "sts:TagSession"]
  }
}

resource "aws_iam_role" "aws_lbc" {
  name               = "${local.resource_prefix}-albc"
  assume_role_policy = data.aws_iam_policy_document.aws_lbc.json
  tags               = var.default_tags
}

# Step 3: Policy from external file (templatefile)
resource "aws_iam_policy" "aws_lbc" {
  policy = templatefile("${path.module}/policies/AWSLoadBalancerController.json", {
    partition = var.partition
  })
  name = "${local.resource_prefix}-albc"
  tags = var.default_tags
}

# Step 4: Attach policy
resource "aws_iam_role_policy_attachment" "aws_lbc" {
  policy_arn = aws_iam_policy.aws_lbc.arn
  role       = aws_iam_role.aws_lbc.name
}

# Step 5: Pod identity association
resource "aws_eks_pod_identity_association" "aws_lbc" {
  cluster_name    = var.cluster_name
  namespace       = "kube-system"                    # System namespace
  service_account = "aws-load-balancer-controller"   # ALBC service account
  role_arn        = aws_iam_role.aws_lbc.arn
}
```

**Kubernetes Requirements:**
- Namespace: `kube-system` (system namespace)
- Service Account: `aws-load-balancer-controller` (created by helm chart)
- Pod Labels: The pod must have the service account

---

### Example 2: Backend Application

**Purpose:** Access Secrets Manager, call AWS Organizations API, assume cross-account role

**Code:**
```hcl
# Step 1 & 2: Trust policy + IAM role
data "aws_iam_policy_document" "backend" {
  statement {
    effect = "Allow"
    principals {
      type        = "Service"
      identifiers = ["pods.eks.amazonaws.com"]
    }
    actions = ["sts:AssumeRole", "sts:TagSession"]
  }
}

resource "aws_iam_role" "backend" {
  name               = "${local.resource_prefix}-backend"
  assume_role_policy = data.aws_iam_policy_document.backend.json
  tags               = var.default_tags
}

# Step 3: Custom policy with specific permissions
resource "aws_iam_policy" "backend" {
  name = "${local.resource_prefix}-backend"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      # Permission 1: Secrets Manager
      {
        Effect = "Allow"
        Action = [
          "secretsmanager:GetSecretValue",
          "secretsmanager:DescribeSecret",
          "secretsmanager:PutSecretValue",
          "secretsmanager:UpdateSecret"
        ]
        Resource = "arn:aws:secretsmanager:${var.region}:${var.account}:secret:${local.resource_prefix}-*"
      },
      # Permission 2: Organizations API
      {
        Effect = "Allow"
        Action = [
          "organizations:ListAccounts",
          "organizations:DescribeAccount"
        ]
        Resource = "*"
      },
      # Permission 3: Cross-account role assumption
      {
        Effect = "Allow"
        Action = ["sts:AssumeRole"]
        Resource = "arn:aws:iam::*:role/OpenOpsReadOnly"
      }
    ]
  })

  tags = var.default_tags
}

# Step 4: Attach policy
resource "aws_iam_role_policy_attachment" "backend" {
  policy_arn = aws_iam_policy.backend.arn
  role       = aws_iam_role.backend.name
}

# Step 5: Pod identity association
resource "aws_eks_pod_identity_association" "backend" {
  cluster_name    = var.cluster_name
  namespace       = "openops"           # Application namespace
  service_account = "backend"           # App-specific service account
  role_arn        = aws_iam_role.backend.arn
}
```

**Kubernetes Requirements:**
- Namespace: `openops` (application namespace)
- Service Account: `backend` (created by application deployment)

---

### Example 3: External Secrets Operator

**Purpose:** Read Secrets Manager and decrypt with KMS

**Code:**
```hcl
# Step 1 & 2: Trust policy + IAM role
data "aws_iam_policy_document" "eso" {
  statement {
    effect = "Allow"
    principals {
      type        = "Service"
      identifiers = ["pods.eks.amazonaws.com"]
    }
    actions = ["sts:AssumeRole", "sts:TagSession"]
  }
}

resource "aws_iam_role" "eso" {
  name               = "${local.resource_prefix}-eso"
  assume_role_policy = data.aws_iam_policy_document.eso.json
  tags               = var.default_tags
}

# Step 3: Policy with resource references
resource "aws_iam_policy" "eso" {
  name = "${local.resource_prefix}-eso"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "secretsmanager:GetSecretValue",
          "secretsmanager:DescribeSecret"
        ]
        Resource = aws_secretsmanager_secret.backend_secrets.arn  # Reference created secret
      },
      {
        Effect = "Allow"
        Action = [
          "kms:Decrypt",
          "kms:DescribeKey"
        ]
        Resource = aws_kms_key.secrets.arn  # Reference created KMS key
      }
    ]
  })

  tags = var.default_tags
}

# Step 4: Attach policy
resource "aws_iam_role_policy_attachment" "eso" {
  policy_arn = aws_iam_policy.eso.arn
  role       = aws_iam_role.eso.name
}

# Step 5: Pod identity association
resource "aws_eks_pod_identity_association" "eso" {
  cluster_name    = var.cluster_name
  namespace       = "external-secrets"      # ESO namespace
  service_account = "external-secrets"      # ESO service account
  role_arn        = aws_iam_role.eso.arn
}
```

**Kubernetes Requirements:**
- Namespace: `external-secrets` (ESO helm chart default)
- Service Account: `external-secrets` (created by ESO helm chart)

---

## Variables Used in eks-addons Module

**Location:** `modules/eks-addons/variables.tf`

```hcl
variable "project_name_alias" {
  description = "The short name of the project"
  type        = string
}

variable "account" {
  description = "AWS account ID"
  type        = string
}

variable "region" {
  description = "AWS region"
  type        = string
}

variable "workspace" {
  description = "Terraform workspace"
  type        = string
}

variable "partition" {
  description = "AWS partition"
  type        = string
  default     = "aws"
}

variable "cluster_name" {
  description = "EKS cluster name"
  type        = string
}

variable "default_tags" {
  description = "Default tags to apply to resources"
  type        = map(string)
  default     = {}
}
```

---

## Module Outputs

**Location:** `modules/eks-addons/outputs.tf`

```hcl
output "aws_load_balancer_controller_role_arn" {
  description = "ARN of the AWS Load Balancer Controller IAM role"
  value       = aws_iam_role.aws_lbc.arn
}

output "backend_secrets_manager_name" {
  description = "Name of the AWS Secrets Manager secret for backend secrets"
  value       = aws_secretsmanager_secret.backend_secrets.name
}

output "backend_secrets_manager_arn" {
  description = "ARN of the AWS Secrets Manager secret for backend secrets"
  value       = aws_secretsmanager_secret.backend_secrets.arn
}
```

---

## How to Add a New Pod Identity Association

To add a new AWS service (e.g., EFS CSI Driver):

### 1. Create Trust Policy Document

```hcl
data "aws_iam_policy_document" "efs_csi" {
  statement {
    effect = "Allow"
    principals {
      type        = "Service"
      identifiers = ["pods.eks.amazonaws.com"]
    }
    actions = ["sts:AssumeRole", "sts:TagSession"]
  }
}
```

### 2. Create IAM Role

```hcl
resource "aws_iam_role" "efs_csi" {
  name               = "${local.resource_prefix}-efs-csi"
  assume_role_policy = data.aws_iam_policy_document.efs_csi.json
  tags               = var.default_tags
}
```

### 3. Attach AWS Managed Policy

```hcl
resource "aws_iam_role_policy_attachment" "efs_csi" {
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonEFSCSIDriverPolicy"
  role       = aws_iam_role.efs_csi.name
}
```

### 4. Create Pod Identity Association

```hcl
resource "aws_eks_pod_identity_association" "efs_csi" {
  cluster_name    = var.cluster_name
  namespace       = "kube-system"
  service_account = "efs-csi-controller-sa"
  role_arn        = aws_iam_role.efs_csi.arn
}
```

### 5. Add Output (Optional)

```hcl
output "efs_csi_role_arn" {
  value = aws_iam_role.efs_csi.arn
}
```

---

## KMS Key & Secrets Manager Setup

### KMS Key for Encryption

```hcl
resource "aws_kms_key" "secrets" {
  description             = "CMK for ${local.resource_prefix} Secrets Manager secrets"
  deletion_window_in_days = 30
  enable_key_rotation     = true
  tags                    = var.default_tags
}

resource "aws_kms_alias" "secrets" {
  name          = "alias/${local.resource_prefix}-secrets"
  target_key_id = aws_kms_key.secrets.key_id
}
```

**Alias Format:** `alias/oops-612674025488-us-west-1-dev-secrets`

### Secrets Manager Secret

```hcl
resource "aws_secretsmanager_secret" "backend_secrets" {
  name       = "${local.resource_prefix}-backend-secrets"
  kms_key_id = aws_kms_key.secrets.arn  # Reference the KMS key ARN
  tags       = var.default_tags
}

resource "aws_secretsmanager_secret_version" "backend_secrets" {
  secret_id     = aws_secretsmanager_secret.backend_secrets.id
  secret_string = "{}"  # Empty initially - managed by ESO
}
```

**Secret Name:** `oops-612674025488-us-west-1-dev-backend-secrets`

---

## How Pods Access AWS Resources

### Flow Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│  KUBERNETES POD                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Pod (backend)                                           │   │
│  │ Service Account: backend                                │   │
│  │ Namespace: openops                                      │   │
│  │                                                         │   │
│  │ Makes call to:                                          │   │
│  │   aws_client.get_secret_value(SecretId="oops-...")    │   │
│  └───────────────────────┬─────────────────────────────────┘   │
│                          │                                      │
│                          │ (pod credentials via Pod Identity)  │
│                          ▼                                      │
└─────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  EKS POD IDENTITY AGENT (runs on node)                          │
│  Intercepts AWS SDK calls                                       │
│  Exchanges pod credentials for temporary AWS role credentials  │
└───────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  AWS STS (Security Token Service)                               │
│  Assumes IAM role: oops-612674025488-us-west-1-dev-backend      │
│  Returns temporary credentials (AccessKey, SecretKey, Token)   │
└───────────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  AWS SERVICES                                                   │
│  Pod makes API calls with temporary credentials                │
│  Services check: Does role have permission?                    │
│  ✓ Secrets Manager - GetSecretValue (allowed)                  │
│  ✗ EC2 - StartInstances (denied)                               │
└─────────────────────────────────────────────────────────────────┘
```

---

## Security Best Practices

1. **Principle of Least Privilege:** Grant only necessary permissions
   ```hcl
   # Bad - too permissive
   "Action": ["*"]
   
   # Good - specific actions
   "Action": [
     "secretsmanager:GetSecretValue",
     "secretsmanager:DescribeSecret"
   ]
   ```

2. **Resource Constraints:** Limit to specific resources
   ```hcl
   # Bad - any secret
   "Resource": "arn:aws:secretsmanager:*:*:secret:*"
   
   # Good - specific secret pattern
   "Resource": "arn:aws:secretsmanager:${var.region}:${var.account}:secret:${local.resource_prefix}-*"
   ```

3. **Separate Roles:** Create distinct roles per service
   ```
   ✓ albc role (for ALB Controller)
   ✓ backend role (for app)
   ✓ eso role (for secrets sync)
   
   ✗ Single shared role (violates least privilege)
   ```

4. **KMS Encryption:** Always encrypt secrets at rest
   ```hcl
   resource "aws_secretsmanager_secret" "example" {
     kms_key_id = aws_kms_key.secrets.arn  # Always set
   }
   ```

5. **Key Rotation:** Enable automatic rotation
   ```hcl
   resource "aws_kms_key" "secrets" {
     enable_key_rotation = true  # Annual automatic rotation
   }
   ```

---

## Troubleshooting

### Check Pod Identity Associations

```bash
aws eks list-pod-identity-associations \
  --cluster-name oops-eks-us-west-1-dev \
  --region us-west-1
```

### Verify IAM Role Trust Policy

```bash
aws iam get-role \
  --role-name oops-612674025488-us-west-1-dev-backend \
  --region us-west-1
```

### Check Pod Logs for Auth Errors

```bash
kubectl logs -n openops deployment/backend
# Look for: "AccessDenied", "UnauthorizedOperation", "AssumeRoleUnauthorizedAccess"
```

### Test Role Assumption

```bash
# Get pod token and test
aws sts assume-role \
  --role-arn arn:aws:iam::612674025488:role/oops-612674025488-us-west-1-dev-backend \
  --role-session-name test-session
```

---

## References

- AWS EKS Pod Identity Documentation
- IAM Policy Examples for AWS Services
- Terraform AWS Provider Documentation
