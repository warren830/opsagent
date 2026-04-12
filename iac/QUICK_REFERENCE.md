# Quick Reference Guide - OpenOps Terraform Infrastructure

## Module Dependency Diagram

```
                          ┌─────────────────────────────┐
                          │   ROOT CONFIGURATION        │
                          │  (main.tf, variables.tf)    │
                          └──────────────┬──────────────┘
                                         │
                ┌────────────────────────┼────────────────────────┐
                │                        │                        │
        ┌───────▼────────┐      ┌───────▼──────────┐     ┌────────▼───────┐
        │   VPC MODULE   │      │  RDS MODULE      │     │ EKS+KARPENTER  │
        │  (Foundation)  │      │ (Depends VPC)    │     │ (Depends VPC)  │
        └───────┬────────┘      └──────────────────┘     └────────┬───────┘
                │                                                 │
                │                     ┌─────────────────────────┐│
                │                     │   EKS-ADDONS MODULE    ││
                │                     │ (Depends VPC + EKS)    ││
                │                     └─────────────────────────┘│
                │                                                 │
                └─────────────────────────────────────────────────┘
                
        ┌──────────────────────────────────────────────────────────┐
        │  OPTIONAL MODULES (No dependencies on others)           │
        │  ├─ Cognito (if enable_cognito = true)                 │
        │  ├─ WAF (if enable_waf = true)                         │
        │  └─ Global Accelerator (if enable_global_accelerator)  │
        └──────────────────────────────────────────────────────────┘
```

---

## Resource Naming Convention

All resources follow this pattern:
```
${project_name_alias}-${account_id}-${region}-${workspace}
```

**Example:** `oops-612674025488-us-west-1-dev`

Breaking down:
- `oops` = project_name_alias
- `612674025488` = AWS account ID
- `us-west-1` = AWS region
- `dev` = Terraform workspace

---

## Key Infrastructure References

### Network Infrastructure

| Component | ID Format | Reference Path |
|-----------|-----------|-----------------|
| VPC ID | vpc-xxxxx | `module.vpc.vpc_id` |
| VPC CIDR | 10.10.0.0/22 | `module.vpc.vpc_cidr_block` |
| Private Subnets | subnet-xxxxx (2x) | `module.vpc.private_subnet_ids` |
| Public Subnets | subnet-xxxxx (2x) | `module.vpc.public_subnet_ids` |
| EKS Nodes SG | sg-xxxxx | `module.vpc.eks_nodes_security_group_id` |
| RDS SG | sg-xxxxx | `module.vpc.rds_security_group_id` |

**Subnet CIDR Allocation:**
- Public Subnet 1: 10.10.0.0/24
- Public Subnet 2: 10.10.1.0/24
- Private Subnet 1: 10.10.2.0/24
- Private Subnet 2: 10.10.3.0/24

---

### EKS Cluster

| Component | Reference |
|-----------|-----------|
| Cluster Name | `oops-eks-us-west-1-dev` |
| Cluster ID | `module.eks_karpenter.cluster_id` |
| API Endpoint | `module.eks_karpenter.cluster_endpoint` |
| Version | `1.32` (configurable) |
| Node Group Name | `oops-dev-612674025488-us-west-1` |
| AMI Type | AL2023_ARM_64_STANDARD |
| Instance Types (dev) | t4g.small |
| Instance Types (prod) | t4g.medium |
| Min Nodes | 2 |
| Max Nodes | 4 |
| Desired Nodes | 2 |

---

### Pod Identity Associations

| Service | Namespace | Service Account | IAM Role |
|---------|-----------|-----------------|----------|
| AWS Load Balancer Controller | kube-system | aws-load-balancer-controller | `{prefix}-albc` |
| Backend Application | openops | backend | `{prefix}-backend` |
| External Secrets Operator | external-secrets | external-secrets | `{prefix}-eso` |
| EBS CSI Driver | kube-system | ebs-csi-controller-sa | Node role |

---

### KMS & Secrets

| Secret | Name | KMS Key | Managed By |
|--------|------|---------|-----------|
| Backend Secrets | `{prefix}-backend-secrets` | `alias/{prefix}-secrets` | Terraform + ESO |
| RDS Master Password | `{prefix}-aurora-postgres-pwd` | `alias/{prefix}-rds-secrets` | Terraform |

**KMS Key Rotation:** Enabled (automatic annual)
**Deletion Window:** 30 days

---

### Database

| Component | Value |
|-----------|-------|
| Engine | Aurora PostgreSQL |
| Database Name | openops |
| Master Username | postgres |
| Master Password | Stored in Secrets Manager |
| Instance Class (dev) | db.r6g.large |
| Instance Class (prod) | db.r6g.large |
| Multi-AZ | No (single instance) |
| Backup Retention (dev) | 1 day |
| Backup Retention (prod) | 7 days |
| Backup Window | 03:00-04:00 UTC |
| Maintenance Window | Sunday 04:00-05:00 UTC |

---

## Security Group Rules

### EKS Nodes Security Group

**Ingress:**
- Protocol: TCP
- Port Range: 0-65535
- Source: Self (same security group)

**Egress:**
- Protocol: All
- Destination: 0.0.0.0/0

---

### RDS Security Group

**Ingress - Port 5432 (PostgreSQL):**
- Source 1: EKS nodes security group
- Source 2: Private subnets (10.10.2.0/24, 10.10.3.0/24)

**Egress:**
- Protocol: All
- Destination: 0.0.0.0/0

---

## Terraform State Management

```hcl
backend "s3" {
  region = "us-west-1"
  bucket = "openops-tfstate-612674025488"
  key    = "openops/tf.state"
}
```

**State File Location:** `s3://openops-tfstate-612674025488/openops/tf.state`

---

## Provider Configuration

```hcl
provider "aws" {
  region = var.region  # Set via terraform.tfvars or environment
  
  default_tags {
    tags = {
      Project     = "openops"
      Environment = terraform.workspace
      ManagedBy   = "terraform"
    }
  }
}
```

---

## Important File Locations

```
/Users/kolya/kolya-projects/openops/iac/
├── main.tf                              # Module composition
├── variables.tf                         # Root variable definitions
├── outputs.tf                           # Root outputs
├── providers.tf                         # AWS provider config
├── terraform.tfvars                     # Variable values (environment-specific)
│
└── modules/
    ├── eks-addons/
    │   ├── main.tf                      # Pod identity, KMS, Secrets Manager
    │   ├── policies/
    │   │   └── AWSLoadBalancerController.json
    │   └── outputs.tf
    │
    ├── eks-karpenter/
    │   ├── eks.tf                       # EKS cluster + addons
    │   ├── karpenter.tf                 # Karpenter configuration
    │   └── outputs.tf
    │
    ├── vpc/
    │   ├── main.tf                      # VPC, subnets, security groups
    │   └── outputs.tf
    │
    └── rds-aurora-postgresql/
        ├── main.tf                      # RDS Aurora cluster
        └── outputs.tf
```

---

## IAM Policies Used

### AWS Managed Policies

| Service | Policy | Path |
|---------|--------|------|
| EBS CSI Driver | AmazonEBSCSIDriverPolicy | service-role |
| EKS Monitoring | AmazonRDSEnhancedMonitoringRole | service-role |
| ECR | AmazonEC2ContainerRegistryReadOnly | N/A |
| Karpenter Node | AmazonEC2ContainerRegistryReadOnly | N/A |

### Custom Policies

| Name | Location | Purpose |
|------|----------|---------|
| ALB Controller | AWSLoadBalancerController.json | ALB/NLB management |
| Backend | Inline in eks-addons/main.tf | Secrets + Organizations |
| ESO | Inline in eks-addons/main.tf | Secrets + KMS decrypt |

---

## Environment-Specific Configurations

### Dev Workspace
- Instance Types: t4g.small
- EBS Volume Size: 30 GB
- Backup Retention: 1 day
- Deletion Protection: No
- Enhanced Monitoring: No
- Performance Insights: Yes

### Prod Workspace
- Instance Types: t4g.medium
- EBS Volume Size: 100 GB
- Backup Retention: 7 days
- Deletion Protection: Yes
- Enhanced Monitoring: Yes (60s interval)
- Performance Insights: Yes
- Cognito Security: ENFORCED
- Copy Tags to Snapshot: Yes

---

## Common Terraform Commands

```bash
# Initialize Terraform (backend setup)
terraform init

# Plan changes for current workspace
terraform plan

# Apply changes
terraform apply

# Switch workspace
terraform workspace select dev
terraform workspace select prod

# Show current state
terraform state list
terraform state show module.vpc

# Output values
terraform output

# Validate configuration
terraform validate

# Format code
terraform fmt -recursive

# Destroy resources (be careful!)
terraform destroy
```

---

## Workspace Management

```bash
# List available workspaces
terraform workspace list

# Create new workspace
terraform workspace new staging

# Switch workspace
terraform workspace select prod

# Current workspace in code
terraform.workspace
```

---

## Troubleshooting Reference

### Check EKS Cluster Status
```bash
aws eks describe-cluster --name oops-eks-us-west-1-dev --region us-west-1
```

### Check Pod Identity Associations
```bash
aws eks list-pod-identity-associations --cluster-name oops-eks-us-west-1-dev --region us-west-1
```

### Verify IAM Role
```bash
aws iam get-role --role-name oops-612674025488-us-west-1-dev-backend
```

### Check KMS Key
```bash
aws kms describe-key --key-id alias/oops-612674025488-us-west-1-dev-secrets --region us-west-1
```

### Check Secrets Manager
```bash
aws secretsmanager get-secret-value --secret-id oops-612674025488-us-west-1-dev-backend-secrets --region us-west-1
```

---

## Key Outputs from Root Module

```hcl
cluster_id                           # EKS cluster ID
cluster_name                         # Cluster name
cluster_endpoint                     # API server endpoint
vpc_id                              # VPC identifier
private_subnet_ids                  # Private subnet IDs (list)
public_subnet_ids                   # Public subnet IDs (list)
eks_nodes_security_group_id         # EKS node security group
rds_security_group_id               # RDS security group
rds_cluster_endpoint                # RDS write endpoint
rds_cluster_reader_endpoint         # RDS read endpoint
backend_secrets_manager_name        # Secrets Manager secret name
karpenter_queue_name                # Karpenter SQS queue
cognito_user_pool_id                # Cognito (if enabled)
waf_web_acl_arn                    # WAF WebACL (if enabled)
```

---

## Notes on EFS CSI Driver Integration

To add EFS CSI driver following the same pattern as EBS CSI:

1. Add addon in `modules/eks-karpenter/eks.tf`:
   ```hcl
   aws-efs-csi-driver = {}
   ```

2. Attach policy to node role:
   ```hcl
   resource "aws_iam_role_policy_attachment" "node_group_efs_csi_policy" {
     role       = module.eks.eks_managed_node_groups["core_node_group"].iam_role_name
     policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonEFSCSIDriverPolicy"
   }
   ```

3. The service account will be `efs-csi-controller-sa` in `kube-system` namespace

