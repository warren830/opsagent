# OpenOps Terraform Infrastructure - Exploration Documentation

This directory contains comprehensive documentation about the OpenOps Terraform infrastructure after a complete exploration. These documents are designed to help you understand the architecture, replicate patterns, and extend the infrastructure.

## 📚 Documentation Files

### 1. **TERRAFORM_INFRASTRUCTURE_REPORT.md** (28 KB)
**Comprehensive reference guide to the entire infrastructure**

Contains:
- Full directory structure of `/iac/` with file descriptions
- Module composition and wiring in `main.tf`
- Deep dive into EKS-Addons module (IAM, Pod Identity, KMS)
- KMS key management details
- VPC module with subnets and security groups
- Security group patterns and configurations
- EKS cluster configuration details
- Critical reference variables and names
- How to replicate EBS CSI pattern for EFS CSI driver
- Summary of key files to reference

**Use this when:** You need detailed information about any infrastructure component.

---

### 2. **QUICK_REFERENCE.md** (11 KB)
**Fast lookup guide with key information in table format**

Contains:
- Module dependency diagram (ASCII art)
- Resource naming conventions
- Network infrastructure reference table
- EKS cluster configuration
- Pod identity associations mapping
- KMS & Secrets reference
- Database configuration
- Security group rules
- IAM policies used
- Environment-specific configurations (dev vs prod)
- Common Terraform commands
- Troubleshooting reference
- Key outputs from root module

**Use this when:** You need to look up a specific value, name, or configuration quickly.

---

### 3. **EKS_ADDONS_POD_IDENTITY_PATTERN.md** (18 KB)
**Detailed reference for the Pod Identity pattern used in EKS add-ons**

Contains:
- Overview of Pod Identity (vs IRSA)
- 5-step pattern structure for adding services
- Real-world examples (ALB Controller, Backend, ESO)
- How to add a new Pod Identity association
- KMS key and Secrets Manager setup
- Pod access flow diagram
- Security best practices
- Troubleshooting commands

**Use this when:** You're implementing EFS CSI driver or other Pod Identity associations, or you need to understand the pattern used.

---

## 🗂️ Directory Structure

```
/Users/kolya/kolya-projects/openops/iac/
├── README_EXPLORATION.md                    ← You are here
├── TERRAFORM_INFRASTRUCTURE_REPORT.md       ← Comprehensive reference
├── QUICK_REFERENCE.md                       ← Fast lookup
├── EKS_ADDONS_POD_IDENTITY_PATTERN.md      ← Pattern details
│
├── main.tf                                  # Root module - module composition
├── variables.tf                             # Root variables
├── outputs.tf                               # Root outputs
├── providers.tf                             # AWS provider config
├── terraform.tfvars                         # Variable values
├── init-backend.sh                          # Backend initialization
│
└── modules/
    ├── eks-addons/                          # Pod identity, KMS, Secrets
    │   ├── main.tf
    │   ├── outputs.tf
    │   ├── variables.tf
    │   └── policies/
    │       └── AWSLoadBalancerController.json
    ├── eks-karpenter/                       # EKS cluster + Karpenter
    │   ├── eks.tf
    │   ├── karpenter.tf
    │   ├── main.tf
    │   ├── outputs.tf
    │   └── variables.tf
    ├── vpc/                                 # VPC, subnets, security groups
    │   ├── main.tf
    │   ├── outputs.tf
    │   └── variables.tf
    ├── rds-aurora-postgresql/               # RDS Aurora cluster
    │   ├── main.tf
    │   ├── outputs.tf
    │   └── variables.tf
    ├── cognito/                             # AWS Cognito (optional)
    │   ├── main.tf
    │   ├── outputs.tf
    │   ├── variables.tf
    │   └── lambda_pre_signup.py
    ├── waf/                                 # AWS WAF (optional)
    │   ├── main.tf
    │   ├── outputs.tf
    │   ├── variables.tf
    │   └── data.tf
    └── global-accelerator/                  # AWS Global Accelerator (optional)
        ├── main.tf
        ├── outputs.tf
        ├── variables.tf
        ├── data.tf
        └── scripts/
```

---

## 🔍 Quick Lookup Guide

### I need to understand...

**The overall architecture**
→ Read: `QUICK_REFERENCE.md` "Module Dependency Diagram" + `TERRAFORM_INFRASTRUCTURE_REPORT.md` Section 2

**How Pod Identity works**
→ Read: `EKS_ADDONS_POD_IDENTITY_PATTERN.md` "Overview" and "Pattern Structure"

**How to add EFS CSI driver**
→ Read: `TERRAFORM_INFRASTRUCTURE_REPORT.md` Section 9 + `EKS_ADDONS_POD_IDENTITY_PATTERN.md` "How to Add a New Pod Identity Association"

**How KMS keys are set up**
→ Read: `TERRAFORM_INFRASTRUCTURE_REPORT.md` Section 4

**Network topology (subnets, security groups)**
→ Read: `TERRAFORM_INFRASTRUCTURE_REPORT.md` Section 5 and 6

**Resource naming patterns**
→ Read: `QUICK_REFERENCE.md` "Resource Naming Convention" or full details in `TERRAFORM_INFRASTRUCTURE_REPORT.md` Section 8

**EKS cluster configuration**
→ Read: `QUICK_REFERENCE.md` "EKS Cluster" table or detailed info in `TERRAFORM_INFRASTRUCTURE_REPORT.md` Section 7

**Differences between dev and prod**
→ Read: `QUICK_REFERENCE.md` "Environment-Specific Configurations"

**How to troubleshoot an issue**
→ Read: `QUICK_REFERENCE.md` "Troubleshooting Reference" or `EKS_ADDONS_POD_IDENTITY_PATTERN.md` "Troubleshooting"

**IAM permissions for a service**
→ Read: `EKS_ADDONS_POD_IDENTITY_PATTERN.md` "Real-World Examples"

**KMS key and Secrets Manager naming**
→ Read: `QUICK_REFERENCE.md` "KMS & Secrets" table

---

## 📋 Key Facts

### Infrastructure Components

| Component | Name/ID | Reference Doc |
|-----------|---------|---------------|
| VPC CIDR | 10.10.0.0/22 | QUICK_REFERENCE.md |
| Private Subnets | 2x (10.10.2.0/24, 10.10.3.0/24) | TERRAFORM_INFRASTRUCTURE_REPORT.md §5 |
| Public Subnets | 2x (10.10.0.0/24, 10.10.1.0/24) | TERRAFORM_INFRASTRUCTURE_REPORT.md §5 |
| EKS Cluster | oops-eks-{region}-{workspace} | QUICK_REFERENCE.md |
| EKS Version | 1.32 | QUICK_REFERENCE.md |
| Node Types (dev) | t4g.small | QUICK_REFERENCE.md |
| Node Types (prod) | t4g.medium | QUICK_REFERENCE.md |
| Min Nodes | 2 | QUICK_REFERENCE.md |
| Max Nodes | 4 | QUICK_REFERENCE.md |
| RDS Engine | Aurora PostgreSQL | QUICK_REFERENCE.md |
| RDS Database | openops | QUICK_REFERENCE.md |

### Important Variables

**Resource Prefix Pattern:**
```
${project_name_alias}-${account_id}-${region}-${workspace}
```

Example: `oops-612674025488-us-west-1-dev`

### Key IAM Roles

| Role | Namespace | Service Account |
|------|-----------|-----------------|
| ALB Controller | kube-system | aws-load-balancer-controller |
| Backend | openops | backend |
| ESO | external-secrets | external-secrets |

### KMS Keys

| Key | Purpose | Alias Pattern |
|-----|---------|---------------|
| EKS Addons KMS | Secrets encryption | alias/{prefix}-secrets |
| RDS KMS | RDS secrets encryption | alias/{prefix}-rds-secrets |

---

## 🚀 Common Tasks

### Task: Add EFS CSI Driver Support

**Steps:**
1. Read: `TERRAFORM_INFRASTRUCTURE_REPORT.md` Section 9
2. Modify: `modules/eks-karpenter/eks.tf`
3. Add the addon and node policy attachment
4. Run `terraform plan` and `terraform apply`

**Details:** See `EKS_ADDONS_POD_IDENTITY_PATTERN.md` for the exact pattern

---

### Task: Add a New AWS Service with Pod Identity

**Steps:**
1. Read: `EKS_ADDONS_POD_IDENTITY_PATTERN.md` "How to Add a New Pod Identity Association"
2. Add to: `modules/eks-addons/main.tf`
3. Follow the 5-step pattern (trust policy, role, policy, attachment, association)
4. Add output if needed
5. Run `terraform plan` and `terraform apply`

---

### Task: Understand a Specific IAM Permission

**Steps:**
1. Find the role in `modules/eks-addons/main.tf`
2. Check the policy document
3. Cross-reference with `EKS_ADDONS_POD_IDENTITY_PATTERN.md` examples
4. Refer to AWS documentation for the specific action

---

### Task: Check Resource Names in AWS

**Reference:** `QUICK_REFERENCE.md` resource naming patterns, then use AWS CLI:

```bash
# Check EKS cluster
aws eks describe-cluster --name oops-eks-us-west-1-dev

# Check IAM role
aws iam get-role --role-name oops-612674025488-us-west-1-dev-backend

# Check KMS key
aws kms describe-key --key-id alias/oops-612674025488-us-west-1-dev-secrets

# Check Secrets Manager secret
aws secretsmanager get-secret-value --secret-id oops-612674025488-us-west-1-dev-backend-secrets
```

---

## 📐 Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        AWS ACCOUNT                              │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    VPC (10.10.0.0/22)                   │   │
│  │  ┌─────────────────────────────────────────────────┐    │   │
│  │  │           Public Subnets (2x)                  │    │   │
│  │  │  10.10.0.0/24         │         10.10.1.0/24  │    │   │
│  │  │  [NAT Gateway]        │                        │    │   │
│  │  └─────────────────────────────────────────────────┘    │   │
│  │  ┌─────────────────────────────────────────────────┐    │   │
│  │  │           Private Subnets (2x)                 │    │   │
│  │  │  10.10.2.0/24         │         10.10.3.0/24  │    │   │
│  │  │                                                 │    │   │
│  │  │  ┌─────────────────┐    ┌──────────────────┐   │    │   │
│  │  │  │   EKS Nodes     │    │   RDS Aurora     │   │    │   │
│  │  │  │  (t4g.small)    │    │   PostgreSQL     │   │    │   │
│  │  │  │  Min: 2, Max: 4 │    │   (db.r6g.large) │   │    │   │
│  │  │  │                 │    │                  │   │    │   │
│  │  │  │ ┌─────────────┐ │    └──────────────────┘   │    │   │
│  │  │  │ │  Karpenter  │ │                           │    │   │
│  │  │  │ │  (Optional) │ │                           │    │   │
│  │  │  │ └─────────────┘ │                           │    │   │
│  │  │  └─────────────────┘                           │    │   │
│  │  └─────────────────────────────────────────────────┘    │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              Kubernetes Add-ons (Pod Identity)          │   │
│  │  ┌─────────────┐  ┌────────────┐  ┌──────────────┐      │   │
│  │  │ ALB         │  │ Backend    │  │ External     │      │   │
│  │  │ Controller  │  │ Service    │  │ Secrets Ops  │      │   │
│  │  │             │  │            │  │              │      │   │
│  │  │ kube-system │  │ openops    │  │ external-    │      │   │
│  │  │             │  │ namespace  │  │ secrets      │      │   │
│  │  └─────────────┘  └────────────┘  └──────────────┘      │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              AWS Services Integration                   │   │
│  │  ┌─────────────────────────────────────────────────┐    │   │
│  │  │  KMS Keys (CMK)                                │    │   │
│  │  │  • alias/{prefix}-secrets (Secrets Manager)    │    │   │
│  │  │  • alias/{prefix}-rds-secrets (RDS)            │    │   │
│  │  └─────────────────────────────────────────────────┘    │   │
│  │  ┌─────────────────────────────────────────────────┐    │   │
│  │  │  Secrets Manager                               │    │   │
│  │  │  • {prefix}-backend-secrets (KMS encrypted)    │    │   │
│  │  │  • {prefix}-aurora-postgres-pwd (KMS enc.)     │    │   │
│  │  └─────────────────────────────────────────────────┘    │   │
│  │  ┌─────────────────────────────────────────────────┐    │   │
│  │  │  IAM Roles (Pod Identity)                      │    │   │
│  │  │  • {prefix}-albc                               │    │   │
│  │  │  • {prefix}-backend                            │    │   │
│  │  │  • {prefix}-eso                                │    │   │
│  │  │  • {prefix}-kpnodeiamrole (Karpenter nodes)    │    │   │
│  │  └─────────────────────────────────────────────────┘    │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 🔧 Terraform State

**Backend:** S3
**Bucket:** openops-tfstate-{account_id}
**Key:** openops/tf.state
**Region:** us-west-1

**Workspace Support:**
- Separate state for each workspace (dev, prod, etc.)
- Switch with: `terraform workspace select {workspace-name}`

---

## ⚠️ Important Notes

### For EFS CSI Driver Implementation

The pattern for EFS CSI driver follows the same approach as EBS CSI driver:

1. **Add addon** in `modules/eks-karpenter/eks.tf`
2. **Attach policy** to node IAM role
3. **Optional:** Create Pod Identity association in `modules/eks-addons/main.tf`

See: `TERRAFORM_INFRASTRUCTURE_REPORT.md` Section 9 and `EKS_ADDONS_POD_IDENTITY_PATTERN.md`

### Security Best Practices

- Always use specific resources in IAM policies (not wildcards)
- Enable KMS key rotation for secrets encryption
- Create separate IAM roles per service (don't share)
- Use IMDSv2 on EC2 instances (enforced in config)
- Encrypt EBS volumes by default

### Workspace Differences

**Dev workspace:**
- Smaller instances (t4g.small)
- Minimal backup retention (1 day)
- No deletion protection
- No enhanced monitoring

**Prod workspace:**
- Larger instances (t4g.medium)
- Extended backups (7 days)
- Full deletion protection
- Enhanced monitoring enabled

---

## 📞 Quick Help

**Where is [resource]?**
→ Search in `QUICK_REFERENCE.md` tables

**How do I [task]?**
→ Check "Common Tasks" section above

**What does [file] contain?**
→ Check "Directory Structure" section

**I found an error, how do I fix it?**
→ See "Troubleshooting Reference" in `QUICK_REFERENCE.md`

**I need to add a new AWS service**
→ Follow the pattern in `EKS_ADDONS_POD_IDENTITY_PATTERN.md`

---

## 📖 File Cross-References

| Topic | Primary Doc | Secondary Doc |
|-------|------------|---------------|
| Architecture Overview | QUICK_REFERENCE.md | TERRAFORM_INFRASTRUCTURE_REPORT.md §1-2 |
| Pod Identity Pattern | EKS_ADDONS_POD_IDENTITY_PATTERN.md | TERRAFORM_INFRASTRUCTURE_REPORT.md §3 |
| KMS Setup | TERRAFORM_INFRASTRUCTURE_REPORT.md §4 | EKS_ADDONS_POD_IDENTITY_PATTERN.md |
| Network & SG | TERRAFORM_INFRASTRUCTURE_REPORT.md §5-6 | QUICK_REFERENCE.md |
| EKS Config | TERRAFORM_INFRASTRUCTURE_REPORT.md §7 | QUICK_REFERENCE.md |
| Variables & Names | TERRAFORM_INFRASTRUCTURE_REPORT.md §8 | QUICK_REFERENCE.md |
| EFS CSI Addition | TERRAFORM_INFRASTRUCTURE_REPORT.md §9 | EKS_ADDONS_POD_IDENTITY_PATTERN.md |

---

**Last Updated:** 2026-04-09
**Explorer:** Claude Code
**Infrastructure:** OpenOps - EKS + Karpenter + RDS Aurora + Pod Identity

