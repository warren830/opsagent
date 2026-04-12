# OpenOps Terraform Infrastructure Exploration Report

## 1. Full Directory Structure of `/iac/`

```
/Users/kolya/kolya-projects/openops/iac/
├── .terraform/                          # Terraform working directory (cached modules)
├── .terraform.lock.hcl                  # Dependency lock file
├── environments/                        # Environment-specific configs (empty)
├── karpenter-node/                      # Karpenter node configuration
├── modules/                             # Main terraform modules
│   ├── cognito/                         # AWS Cognito authentication
│   │   ├── main.tf
│   │   ├── outputs.tf
│   │   ├── variables.tf
│   │   ├── lambda_pre_signup.py
│   │   └── lambda_pre_signup.zip
│   ├── eks-addons/                      # EKS add-ons (ALB, ESO, Secrets)
│   │   ├── main.tf
│   │   ├── outputs.tf
│   │   ├── variables.tf
│   │   └── policies/
│   │       └── AWSLoadBalancerController.json
│   ├── eks-karpenter/                   # EKS cluster + Karpenter
│   │   ├── eks.tf                       # EKS module instantiation
│   │   ├── karpenter.tf                 # Karpenter module instantiation
│   │   ├── main.tf
│   │   ├── outputs.tf
│   │   ├── variables.tf
│   │   └── .terraform/                  # Nested module cache (terraform-aws-modules)
│   ├── global-accelerator/              # AWS Global Accelerator (optional)
│   │   ├── main.tf
│   │   ├── outputs.tf
│   │   ├── variables.tf
│   │   ├── data.tf
│   │   └── scripts/
│   ├── rds-aurora-postgresql/           # RDS Aurora PostgreSQL cluster
│   │   ├── main.tf
│   │   ├── outputs.tf
│   │   └── variables.tf
│   ├── vpc/                             # VPC, subnets, security groups
│   │   ├── main.tf
│   │   ├── outputs.tf
│   │   └── variables.tf
│   └── waf/                             # AWS WAF (optional)
│       ├── main.tf
│       ├── outputs.tf
│       ├── variables.tf
│       └── data.tf
├── main.tf                              # Root module - module composition
├── outputs.tf                           # Root level outputs
├── variables.tf                         # Root level variables
├── providers.tf                         # Terraform providers config
├── providers.tf.template                # Template for provider config
├── terraform.tfvars                     # Terraform variables (values)
└── init-backend.sh                      # S3 backend initialization script
```

---

## 2. Main.tf - Module Composition & Wiring

**File:** `/Users/kolya/kolya-projects/openops/iac/main.tf`

### Key Locals
```hcl
locals {
  account = var.account
  region  = var.region
  workspace       = terraform.workspace
  deployment_name = "${var.project_name}-${local.workspace}"
  eks_version     = var.eks_version
  cluster_name    = "${var.project_name_alias}-eks-${local.region}-${local.workspace}"

  default_tags = {
    "DeploymentName" = local.deployment_name
    "Workspace"      = local.workspace
    "ManagedBy"      = "terraform"
    "Project"        = "openops"
    "Repository"     = "https://github.com/kolya-amazon/openops"
  }
}
```

### Module Instantiation Order & Dependencies

#### 1. **VPC Module** (Foundation - No dependencies)
```hcl
module "vpc" {
  source = "./modules/vpc"
  
  name_prefix  = local.deployment_name
  vpc_cidr     = var.vpc_cidr  # default: "10.10.0.0/22"
  tags         = local.default_tags
  cluster_name = local.cluster_name
}
```
**Key Outputs Used:**
- `vpc_id` - VPC identifier
- `private_subnet_ids` - List of 2 private subnets for EKS
- `public_subnet_ids` - List of 2 public subnets
- `eks_nodes_security_group_id` - Security group for EKS nodes
- `rds_security_group_id` - Security group for RDS

#### 2. **RDS Aurora PostgreSQL Module** (Depends on VPC)
```hcl
module "rds_aurora_postgresql" {
  source = "./modules/rds-aurora-postgresql"
  
  project_name_alias = var.project_name_alias
  workspace          = local.workspace
  account            = local.account
  region             = local.region
  
  # Network from VPC module
  vpc_id             = module.vpc.vpc_id
  subnet_ids         = module.vpc.private_subnet_ids
  security_group_ids = [module.vpc.rds_security_group_id]
  
  instance_count = 1
  storage_encrypted = true
  kms_key_id = ""  # Uses module-created KMS key
  
  # Workspace-dependent settings
  deletion_protection = local.workspace == "prod" ? true : false
  backup_retention_period = local.workspace == "prod" ? 7 : 1
  # ... more prod-specific configs
}
```
**Key Outputs Used:**
- `cluster_endpoint` - RDS endpoint
- `cluster_reader_endpoint` - Read-only endpoint
- `secret_name` - Secrets Manager secret name (master password)

#### 3. **EKS + Karpenter Module** (Depends on VPC)
```hcl
module "eks_karpenter" {
  source = "./modules/eks-karpenter"
  
  project_name_alias = var.project_name_alias
  workspace          = local.workspace
  account            = local.account
  region             = local.region
  
  cluster_name       = local.cluster_name
  kubernetes_version = local.eks_version  # default: "1.32"
  
  # Network from VPC module
  vpc_id             = module.vpc.vpc_id
  subnet_ids         = module.vpc.private_subnet_ids
  additional_security_group_ids = [module.vpc.eks_nodes_security_group_id]
  
  default_tags = local.default_tags
}
```
**Key Outputs Used:**
- `cluster_id` - EKS cluster ID
- `cluster_name` - EKS cluster name
- `cluster_endpoint` - Kubernetes API endpoint
- `cluster_security_group_id` - EKS control plane security group
- `karpenter_queue_name` - SQS queue name for node scaling
- `karpenter_node_iam_role_name` - IAM role for Karpenter nodes

#### 4. **EKS Add-ons Module** (Depends on EKS + VPC)
```hcl
module "eks_addons" {
  source = "./modules/eks-addons"
  
  project_name_alias = var.project_name_alias
  account            = local.account
  region             = local.region
  workspace          = local.workspace
  
  cluster_name = module.eks_karpenter.cluster_name
  default_tags = local.default_tags
  
  depends_on = [module.eks_karpenter, module.vpc]
}
```
**Creates:** Pod identity associations, IAM roles, KMS keys for secrets

#### 5. **Cognito Module** (Optional - No dependencies)
```hcl
module "cognito" {
  count  = var.enable_cognito ? 1 : 0  # Only if enabled
  source = "./modules/cognito"
  
  # OAuth configuration
  callback_urls = var.frontend_domain != "" ? [
    "https://${var.frontend_domain}/auth/cognito/callback",
    "http://localhost:3000/auth/cognito/callback"
  ] : var.cognito_callback_urls
}
```

#### 6. **WAF Module** (Optional - No dependencies)
```hcl
module "waf" {
  count  = var.enable_waf ? 1 : 0  # Only if enabled
  source = "./modules/waf"
  
  frontend_alb_name = var.waf_frontend_alb_name
  api_alb_name      = var.waf_api_alb_name
  rate_limit_global = var.waf_rate_limit_global  # default: 2000
  rate_limit_auth   = var.waf_rate_limit_auth    # default: 20
  rate_limit_chat   = var.waf_rate_limit_chat    # default: 300
}
```

---

## 3. EKS-Addons Module - Deep Dive (IAM, Pod Identity, KMS)

**Files:** 
- `/Users/kolya/kolya-projects/openops/iac/modules/eks-addons/main.tf`
- `/Users/kolya/kolya-projects/openops/iac/modules/eks-addons/variables.tf`
- `/Users/kolya/kolya-projects/openops/iac/modules/eks-addons/outputs.tf`
- `/Users/kolya/kolya-projects/openops/iac/modules/eks-addons/policies/AWSLoadBalancerController.json`

### Resource Prefix Pattern
```hcl
locals {
  resource_prefix = "${var.project_name_alias}-${var.account}-${var.region}-${var.workspace}"
}
# Example: "oops-612674025488-us-west-1-dev"
```

### 1. AWS Load Balancer Controller (ALB Controller)

#### Pod Identity Association
```hcl
data "aws_iam_policy_document" "aws_lbc" {
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

resource "aws_iam_role" "aws_lbc" {
  name               = "${local.resource_prefix}-albc"
  assume_role_policy = data.aws_iam_policy_document.aws_lbc.json
  tags               = var.default_tags
}
```

#### IAM Policy
**Location:** `modules/eks-addons/policies/AWSLoadBalancerController.json`

**Key Permissions:**
- `iam:CreateServiceLinkedRole` - For ELB service
- **EC2 Describe Permissions:** AccountAttributes, Addresses, AvailabilityZones, InternetGateways, VPCs, Subnets, SecurityGroups, Instances, NetworkInterfaces, Tags
- **ELB Permissions:** DescribeLoadBalancers, DescribeTargetGroups, DescribeListeners, DescribeRules, DescribeTags
- **Modify Operations:** AuthorizeSecurityGroupIngress, RevokeSecurityGroupIngress, CreateLoadBalancer, CreateTargetGroup, ModifyListener, RegisterTargets, etc.
- **Security:** WAF/WAFv2 association, Shield protection
- **ACM/Certificates:** ListCertificates, DescribeCertificate

```hcl
resource "aws_iam_policy" "aws_lbc" {
  policy = templatefile("${path.module}/policies/AWSLoadBalancerController.json", {
    partition = var.partition  # default: "aws"
  })
  name = "${local.resource_prefix}-albc"
  tags = var.default_tags
}

resource "aws_iam_role_policy_attachment" "aws_lbc" {
  policy_arn = aws_iam_policy.aws_lbc.arn
  role       = aws_iam_role.aws_lbc.name
}
```

#### Pod Identity Association (EKS Native)
```hcl
resource "aws_eks_pod_identity_association" "aws_lbc" {
  cluster_name    = var.cluster_name
  namespace       = "kube-system"
  service_account = "aws-load-balancer-controller"
  role_arn        = aws_iam_role.aws_lbc.arn
}
```
**Key Pattern:**
- Uses EKS Pod Identity (not IRSA)
- Namespace: `kube-system`
- Service Account: `aws-load-balancer-controller`
- Role: Assumed via `pods.eks.amazonaws.com` principal

---

### 2. Backend Service (Pod Identity Association)

#### Pod Identity Setup
```hcl
data "aws_iam_policy_document" "backend" {
  statement {
    effect = "Allow"
    principals {
      type        = "Service"
      identifiers = ["pods.eks.amazonaws.com"]
    }
    actions = [
      "sts:AssumeRole",
      "sts:TagSession"
    ]
  }
}

resource "aws_iam_role" "backend" {
  name               = "${local.resource_prefix}-backend"
  assume_role_policy = data.aws_iam_policy_document.backend.json
  tags               = var.default_tags
}
```

#### IAM Policy (Secrets Manager + Organizations Access)
```hcl
resource "aws_iam_policy" "backend" {
  name = "${local.resource_prefix}-backend"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
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
      {
        Effect = "Allow"
        Action = [
          "organizations:ListAccounts",
          "organizations:DescribeAccount"
        ]
        Resource = "*"
      },
      {
        Effect   = "Allow"
        Action   = ["sts:AssumeRole"]
        Resource = "arn:aws:iam::*:role/OpenOpsReadOnly"
      }
    ]
  })

  tags = var.default_tags
}

resource "aws_eks_pod_identity_association" "backend" {
  cluster_name    = var.cluster_name
  namespace       = "openops"           # App namespace
  service_account = "backend"            # App service account
  role_arn        = aws_iam_role.backend.arn
}
```

**Key Permissions:**
- Secrets Manager: Read/write/update secrets matching pattern `${resource_prefix}-*`
- Organizations: List and describe accounts
- Cross-account AssumeRole: `OpenOpsReadOnly` role in any account

---

### 3. KMS Key for Secrets Manager Encryption

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

**Key Properties:**
- **Name Format:** `alias/oops-612674025488-us-west-1-dev-secrets`
- **Key ID:** Generated by AWS
- **Rotation:** Enabled (automatic annual rotation)
- **Deletion Window:** 30 days (safe deletion)

---

### 4. Backend Secrets Storage (Secrets Manager)

```hcl
resource "aws_secretsmanager_secret" "backend_secrets" {
  name       = "${local.resource_prefix}-backend-secrets"
  kms_key_id = aws_kms_key.secrets.arn
  tags       = var.default_tags
}

resource "aws_secretsmanager_secret_version" "backend_secrets" {
  secret_id     = aws_secretsmanager_secret.backend_secrets.id
  secret_string = "{}"  # Empty - managed by External Secrets Operator
}
```

**Naming:** `oops-612674025488-us-west-1-dev-backend-secrets`
**Encryption:** KMS CMK (same key as above)
**Management:** Managed by External Secrets Operator (ESO)

---

### 5. External Secrets Operator (ESO) Pod Identity

```hcl
data "aws_iam_policy_document" "eso" {
  statement {
    effect = "Allow"
    principals {
      type        = "Service"
      identifiers = ["pods.eks.amazonaws.com"]
    }
    actions = [
      "sts:AssumeRole",
      "sts:TagSession"
    ]
  }
}

resource "aws_iam_role" "eso" {
  name               = "${local.resource_prefix}-eso"
  assume_role_policy = data.aws_iam_policy_document.eso.json
  tags               = var.default_tags
}

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
        Resource = aws_secretsmanager_secret.backend_secrets.arn
      },
      {
        Effect = "Allow"
        Action = [
          "kms:Decrypt",
          "kms:DescribeKey"
        ]
        Resource = aws_kms_key.secrets.arn
      }
    ]
  })

  tags = var.default_tags
}

resource "aws_iam_role_policy_attachment" "eso" {
  policy_arn = aws_iam_policy.eso.arn
  role       = aws_iam_role.eso.name
}

resource "aws_eks_pod_identity_association" "eso" {
  cluster_name    = var.cluster_name
  namespace       = "external-secrets"      # ESO namespace
  service_account = "external-secrets"      # ESO service account
  role_arn        = aws_iam_role.eso.arn
}
```

**Key Permissions:**
- Secrets Manager: Get and describe the backend secrets secret
- KMS: Decrypt the secret and describe the key

---

### EKS-Addons Module Outputs

```hcl
output "aws_load_balancer_controller_role_arn" {
  value = aws_iam_role.aws_lbc.arn
}

output "backend_secrets_manager_name" {
  value = aws_secretsmanager_secret.backend_secrets.name
  # Example: "oops-612674025488-us-west-1-dev-backend-secrets"
}

output "backend_secrets_manager_arn" {
  value = aws_secretsmanager_secret.backend_secrets.arn
}
```

---

## 4. KMS Key Management

### KMS Keys Created by Terraform

#### 1. **EKS Add-ons Module KMS Key**
**Location:** `/Users/kolya/kolya-projects/openops/iac/modules/eks-addons/main.tf:133-144`

```hcl
resource "aws_kms_key" "secrets" {
  description             = "CMK for ${resource_prefix} Secrets Manager secrets"
  deletion_window_in_days = 30
  enable_key_rotation     = true
  tags                    = var.default_tags
}

resource "aws_kms_alias" "secrets" {
  name          = "alias/${local.resource_prefix}-secrets"
  target_key_id = aws_kms_key.secrets.key_id
}
```

**Naming Convention:**
- **Alias:** `alias/oops-{account_id}-{region}-{workspace}-secrets`
- **Example:** `alias/oops-612674025488-us-west-1-dev-secrets`

**Reference Variable Names:**
- `aws_kms_key.secrets.key_id` - Key ID (e.g., `12345678-1234-1234-1234-123456789012`)
- `aws_kms_key.secrets.arn` - Key ARN

#### 2. **RDS Module KMS Key** (Separate)
**Location:** `/Users/kolya/kolya-projects/openops/iac/modules/rds-aurora-postgresql/main.tf:46-56`

```hcl
resource "aws_kms_key" "secrets" {
  description             = "CMK for ${local.resource_prefix} RDS Secrets Manager"
  deletion_window_in_days = 30
  enable_key_rotation     = true
  tags                    = var.default_tags
}

resource "aws_kms_alias" "secrets" {
  name          = "alias/${local.resource_prefix}-rds-secrets"
  target_key_id = aws_kms_key.secrets.key_id
}
```

**Naming Convention:**
- **Alias:** `alias/oops-{account_id}-{region}-{workspace}-rds-secrets`

### How EBS CSI Driver KMS Integration Works (Pattern to Replicate)

The EBS CSI driver isn't explicitly configured in Terraform because:
1. **EBS CSI Addon:** Installed via EKS addon in `eks.tf`:
   ```hcl
   addons = {
     aws-ebs-csi-driver = {}
   }
   ```

2. **Node IAM Role Permission:** EBS CSI policy attached to core node group:
   ```hcl
   resource "aws_iam_role_policy_attachment" "node_group_ebs_csi_policy" {
     role       = module.eks.eks_managed_node_groups["core_node_group"].iam_role_name
     policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonEBSCSIDriverPolicy"
   }
   ```

3. **KMS Encryption:** EBS volumes can be encrypted using default AWS-managed key or custom CMK. Custom KMS would require policy update to grant `kms:Decrypt`, `kms:GenerateDataKey` to the node role.

---

## 5. VPC Module - Network & Security Group Details

**File:** `/Users/kolya/kolya-projects/openops/iac/modules/vpc/main.tf`

### Subnets Configuration

```hcl
# Public Subnets (2 across AZs)
resource "aws_subnet" "public" {
  count = 2
  
  vpc_id                  = aws_vpc.main.id
  cidr_block              = cidrsubnet(var.vpc_cidr, 2, count.index)
  availability_zone       = data.aws_availability_zones.available.names[count.index]
  map_public_ip_on_launch = false
  
  tags = {
    Name                     = "${var.name_prefix}-public-subnet-{1..2}"
    Type                     = "Public"
    "kubernetes.io/role/elb" = "1"  # ALB discovery
  }
}

# Private Subnets (2 across AZs)
resource "aws_subnet" "private" {
  count = 2
  
  vpc_id            = aws_vpc.main.id
  cidr_block        = cidrsubnet(var.vpc_cidr, 2, count.index + 2)
  availability_zone = data.aws_availability_zones.available.names[count.index]
  
  tags = {
    Name                              = "${var.name_prefix}-private-subnet-{1..2}"
    Type                              = "Private"
    "kubernetes.io/role/internal-elb" = "1"  # Internal LB discovery
    "karpenter.sh/discovery"          = var.cluster_name  # Karpenter discovery
  }
}
```

**CIDR Subnet Allocation (from 10.10.0.0/22):**
- Public-1: `10.10.0.0/24` (indices 0)
- Public-2: `10.10.1.0/24` (indices 1)
- Private-1: `10.10.2.0/24` (indices 2)
- Private-2: `10.10.3.0/24` (indices 3)

### VPC Module Outputs

```hcl
output "vpc_id" { value = aws_vpc.main.id }
output "vpc_cidr_block" { value = aws_vpc.main.cidr_block }
output "private_subnet_ids" { value = aws_subnet.private[*].id }
output "public_subnet_ids" { value = aws_subnet.public[*].id }
output "internet_gateway_id" { value = aws_internet_gateway.main.id }
output "nat_gateway_id" { value = aws_nat_gateway.main.id }
output "public_route_table_id" { value = aws_route_table.public.id }
output "private_route_table_id" { value = aws_route_table.private.id }
output "eks_nodes_security_group_id" { value = aws_security_group.eks_nodes.id }
output "rds_security_group_id" { value = aws_security_group.rds.id }
```

---

## 6. Security Group Patterns

### EKS Nodes Security Group

**File:** `modules/vpc/main.tf:137-164`

```hcl
resource "aws_security_group" "eks_nodes" {
  name_prefix = "${var.name_prefix}-eks-nodes-"
  vpc_id      = aws_vpc.main.id

  # Node-to-node communication
  ingress {
    description = "Node to node communication"
    from_port   = 0
    to_port     = 65535
    protocol    = "tcp"
    self        = true  # Allow from same security group
  }

  # All outbound traffic
  egress {
    description = "All outbound traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "${var.name_prefix}-eks-nodes-sg"
  }

  lifecycle {
    create_before_destroy = true
  }
}
```

**Security Pattern:**
- **Ingress:** Node-to-node on all TCP ports (0-65535)
- **Egress:** All traffic outbound
- **Self-reference:** Uses `self = true` for node-to-node communication

---

### RDS Security Group

**File:** `modules/vpc/main.tf:167-204`

```hcl
resource "aws_security_group" "rds" {
  name_prefix = "${var.name_prefix}-rds-"
  vpc_id      = aws_vpc.main.id

  # Allow EKS nodes to access PostgreSQL
  ingress {
    description     = "PostgreSQL from EKS nodes"
    from_port       = 5432
    to_port         = 5432
    protocol        = "tcp"
    security_groups = [aws_security_group.eks_nodes.id]
  }

  # Allow private subnets to access PostgreSQL
  ingress {
    description = "PostgreSQL from private subnets"
    from_port   = 5432
    to_port     = 5432
    protocol    = "tcp"
    cidr_blocks = [for subnet in aws_subnet.private : subnet.cidr_block]
  }

  egress {
    description = "All outbound traffic"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "${var.name_prefix}-rds-sg"
  }

  lifecycle {
    create_before_destroy = true
  }
}
```

**Security Pattern:**
- **Ingress - PostgreSQL (5432):**
  - From EKS nodes (security group reference)
  - From private subnets (CIDR blocks - 10.10.2.0/24, 10.10.3.0/24)
- **Egress:** All traffic outbound

**Security Group Attachment in main.tf:**
```hcl
security_group_ids = [module.vpc.rds_security_group_id]
```

---

## 7. EKS Cluster Configuration Details

**File:** `/Users/kolya/kolya-projects/openops/iac/modules/eks-karpenter/eks.tf`

### Core Node Group Configuration

```hcl
eks_managed_node_groups = {
  core_node_group = {
    use_name_prefix          = false
    iam_role_use_name_prefix = false
    name                     = "${var.project_name_alias}-${var.workspace}-${var.account}-${var.region}"
    
    # Instance Configuration
    ami_type       = "AL2023_ARM_64_STANDARD"  # ARM-based (graviton)
    instance_types = var.workspace == "prod" ? ["t4g.medium"] : ["t4g.small"]
    
    # Capacity Configuration
    min_size     = 2
    max_size     = 4
    desired_size = 2
    capacity_type = "ON_DEMAND"
    
    # Metadata Options (IMDSv2 enforced)
    metadata_options = {
      http_endpoint               = "enabled"
      http_tokens                 = "required"        # Enforce IMDSv2
      http_put_response_hop_limit = 2
      instance_metadata_tags      = "enabled"
    }
    
    # EBS Configuration
    ebs_optimized = true
    block_device_mappings = {
      xvda = {
        device_name = "/dev/xvda"
        ebs = {
          volume_size           = var.workspace == "prod" ? 100 : 30
          volume_type           = "gp3"
          encrypted             = true
          delete_on_termination = true
        }
      }
    }
    
    # Security & IAM
    vpc_security_group_ids = var.additional_security_group_ids
    
    # Labels
    labels = {
      WorkerType    = "ON_DEMAND"
      NodeGroupType = "core"
    }
    
    tags = var.default_tags
  }
}
```

**Key Characteristics:**
- **Node Naming:** `oops-dev-612674025488-us-west-1`
- **EBS CSI Driver:** Attached via addon + IAM policy
- **IMDSv2:** Enforced (security best practice)
- **Encryption:** EBS volumes encrypted by default

### EKS Add-ons Configuration

```hcl
addons = {
  coredns = {}
  eks-pod-identity-agent = {
    before_compute = true  # Install before compute nodes
  }
  kube-proxy = {
    before_compute = true
  }
  vpc-cni = {
    before_compute = true
  }
  aws-ebs-csi-driver = {}  # EBS CSI Driver
}
```

---

## 8. Critical Reference Variables & Names

### Resource Naming Pattern
```
${project_name_alias}-${account_id}-${region}-${workspace}
```

**Example for dev environment:**
- `project_name_alias = "oops"`
- `account = "612674025488"`
- `region = "us-west-1"`
- `workspace = "dev"`
- **Result:** `oops-612674025488-us-west-1-dev`

### Important Variable Names

| Variable | Path | Default | Used For |
|----------|------|---------|----------|
| `cluster_name` | main.tf locals | `oops-eks-us-west-1-dev` | EKS cluster identifier |
| `vpc_cidr` | variables.tf | `10.10.0.0/22` | VPC CIDR block |
| `eks_version` | variables.tf | `1.32` | Kubernetes version |
| `workspace` | main.tf locals | Terraform workspace | Environment (dev/prod) |
| `project_name_alias` | variables.tf | `oops` | Short name for resources |

### Key IAM Role Names

| Role | Naming Pattern | Resource Prefix |
|------|----------------|-----------------|
| ALB Controller | `{prefix}-albc` | `oops-612674025488-us-west-1-dev-albc` |
| Backend Service | `{prefix}-backend` | `oops-612674025488-us-west-1-dev-backend` |
| External Secrets Operator | `{prefix}-eso` | `oops-612674025488-us-west-1-dev-eso` |
| Karpenter Node IAM | `{prefix}-kpnodeiamrole` | `oops-612674025488-us-west-1-dev-kpnodeiamrole` |
| Karpenter Controller | `{prefix}-kp-controller-{ws}-{acct}-{region}` | `oops-kp-controller-dev-612674025488-us-west-1` |

### Key Kubernetes Resources

| Namespace | Service Account | Pod Identity Role | Purpose |
|-----------|-----------------|-------------------|---------|
| `kube-system` | `aws-load-balancer-controller` | `{prefix}-albc` | ALB/NLB management |
| `openops` | `backend` | `{prefix}-backend` | Backend application access to Secrets Manager |
| `external-secrets` | `external-secrets` | `{prefix}-eso` | External Secrets Operator |

---

## 9. How to Replicate EBS CSI Pattern for EFS CSI Driver

**Current EBS CSI Setup:**

1. **Addon Installation** (eks.tf):
   ```hcl
   aws-ebs-csi-driver = {}
   ```

2. **Node IAM Policy** (eks.tf):
   ```hcl
   resource "aws_iam_role_policy_attachment" "node_group_ebs_csi_policy" {
     role       = module.eks.eks_managed_node_groups["core_node_group"].iam_role_name
     policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonEBSCSIDriverPolicy"
   }
   ```

**To Replicate for EFS CSI Driver:**

1. **Add EFS CSI Addon** (eks.tf):
   ```hcl
   addons = {
     # ... existing addons ...
     aws-efs-csi-driver = {}
   }
   ```

2. **Attach EFS CSI Policy to Node Role** (eks.tf):
   ```hcl
   resource "aws_iam_role_policy_attachment" "node_group_efs_csi_policy" {
     role       = module.eks.eks_managed_node_groups["core_node_group"].iam_role_name
     policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonEFSCSIDriverPolicy"
   }
   ```

3. **Optional: Create Pod Identity Association** (eks-addons/main.tf):
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

   resource "aws_iam_role" "efs_csi" {
     name               = "${local.resource_prefix}-efs-csi"
     assume_role_policy = data.aws_iam_policy_document.efs_csi.json
     tags               = var.default_tags
   }

   resource "aws_iam_policy_attachment" "efs_csi" {
     policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonEFSCSIDriverPolicy"
     role       = aws_iam_role.efs_csi.name
   }

   resource "aws_eks_pod_identity_association" "efs_csi" {
     cluster_name    = var.cluster_name
     namespace       = "kube-system"
     service_account = "efs-csi-controller-sa"
     role_arn        = aws_iam_role.efs_csi.arn
   }
   ```

---

## Summary: Files to Reference

| Purpose | File Path |
|---------|-----------|
| Module Wiring | `/Users/kolya/kolya-projects/openops/iac/main.tf` |
| Pod Identity (ALB, Backend, ESO) | `/Users/kolya/kolya-projects/openops/iac/modules/eks-addons/main.tf` |
| KMS Key Setup | `/Users/kolya/kolya-projects/openops/iac/modules/eks-addons/main.tf:133-144` |
| ALB Controller Policy | `/Users/kolya/kolya-projects/openops/iac/modules/eks-addons/policies/AWSLoadBalancerController.json` |
| VPC & Subnets | `/Users/kolya/kolya-projects/openops/iac/modules/vpc/main.tf` |
| Security Groups | `/Users/kolya/kolya-projects/openops/iac/modules/vpc/main.tf:137-204` |
| EKS & Karpenter | `/Users/kolya/kolya-projects/openops/iac/modules/eks-karpenter/eks.tf` |
| Karpenter Config | `/Users/kolya/kolya-projects/openops/iac/modules/eks-karpenter/karpenter.tf` |
| RDS Aurora | `/Users/kolya/kolya-projects/openops/iac/modules/rds-aurora-postgresql/main.tf` |

