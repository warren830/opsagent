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
    "Project"        = "ops"
    "Repository"     = "https://github.com/kolya-amazon/ops"
  }
}

# ── VPC ──────────────────────────────────────────────────────
module "vpc" {
  source = "./modules/vpc"

  name_prefix  = local.deployment_name
  vpc_cidr     = var.vpc_cidr
  tags         = local.default_tags
  cluster_name = local.cluster_name
}

# ── RDS Aurora PostgreSQL ────────────────────────────────────
module "rds_aurora_postgresql" {
  source = "./modules/rds-aurora-postgresql"

  # Project configuration
  project_name_alias = var.project_name_alias
  workspace          = local.workspace
  account            = local.account
  region             = local.region

  # Network configuration - using private subnets for security
  vpc_id             = module.vpc.vpc_id
  subnet_ids         = module.vpc.private_subnet_ids
  security_group_ids = [module.vpc.rds_security_group_id]

  # Instance configuration
  instance_count = 1

  # Security settings (configurable for different environments)
  storage_encrypted                   = true
  kms_key_id                          = ""
  iam_database_authentication_enabled = true
  deletion_protection                 = local.workspace == "prod" ? true : false

  # Backup settings
  backup_retention_period = local.workspace == "prod" ? 7 : 1
  preferred_backup_window = local.workspace == "prod" ? "03:00-04:00" : null
  copy_tags_to_snapshot   = local.workspace == "prod" ? true : false
  skip_final_snapshot     = local.workspace == "prod" ? false : true

  # Maintenance settings
  preferred_maintenance_window = "sun:04:00-sun:05:00"
  apply_immediately            = local.workspace == "prod" ? false : true
  auto_minor_version_upgrade   = true

  # Logging settings (only for production)
  enabled_cloudwatch_logs_exports = local.workspace == "prod" ? ["postgresql"] : []

  # Monitoring settings (only for production)
  monitoring_interval = local.workspace == "prod" ? 60 : 0

  # Performance Insights
  performance_insights_enabled = true

  # Tags
  default_tags = local.default_tags
}

# ── EKS + Karpenter ─────────────────────────────────────────
module "eks_karpenter" {
  source = "./modules/eks-karpenter"

  # Project configuration
  project_name_alias = var.project_name_alias
  workspace          = local.workspace
  account            = local.account
  region             = local.region

  # EKS configuration
  cluster_name       = local.cluster_name
  kubernetes_version = local.eks_version
  vpc_id             = module.vpc.vpc_id
  subnet_ids         = module.vpc.private_subnet_ids

  # Additional security groups for EKS nodes
  additional_security_group_ids = [module.vpc.eks_nodes_security_group_id]

  # Tags
  default_tags = local.default_tags
}

# ── EKS Add-ons (ALB Controller, ESO, Secrets) ──────────────
module "eks_addons" {
  source = "./modules/eks-addons"

  # Project configuration
  project_name_alias = var.project_name_alias
  account            = local.account
  region             = local.region
  workspace          = local.workspace

  # EKS cluster information
  cluster_name = module.eks_karpenter.cluster_name

  # Tags
  default_tags = local.default_tags

  depends_on = [module.eks_karpenter, module.vpc]
}

# ── EFS (Workspace persistent storage) ──────────────────────
module "efs" {
  source = "./modules/efs"

  project_name_alias = var.project_name_alias
  account            = local.account
  region             = local.region
  workspace          = local.workspace

  vpc_id                      = module.vpc.vpc_id
  private_subnet_ids          = module.vpc.private_subnet_ids
  eks_nodes_security_group_id = module.vpc.eks_nodes_security_group_id
  eks_node_security_group_id  = module.eks_karpenter.node_security_group_id

  default_tags = local.default_tags

  depends_on = [module.vpc, module.eks_karpenter]
}

# ── Cognito (Optional) ──────────────────────────────────────
module "cognito" {
  count  = var.enable_cognito ? 1 : 0
  source = "./modules/cognito"

  project_name_alias = var.project_name_alias
  workspace          = local.workspace
  account            = local.account
  region             = local.region

  # OAuth URLs: use frontend_domain if set, otherwise defaults
  callback_urls = var.frontend_domain != "" ? [
    "https://${var.frontend_domain}/auth/cognito/callback",
    "http://localhost:3000/auth/cognito/callback"
  ] : var.cognito_callback_urls

  logout_urls = var.frontend_domain != "" ? [
    "https://${var.frontend_domain}/",
    "http://localhost:3000/"
  ] : var.cognito_logout_urls

  access_token_validity  = var.cognito_access_token_validity
  id_token_validity      = var.cognito_id_token_validity
  refresh_token_validity = var.cognito_refresh_token_validity

  # Security settings based on workspace
  advanced_security_mode = local.workspace == "prod" ? "ENFORCED" : "AUDIT"
  deletion_protection    = local.workspace == "prod" ? true : false

  allowed_email_domains = var.cognito_allowed_email_domains

  default_tags = local.default_tags
}

# ── WAF (Optional) ──────────────────────────────────────────
# NOTE: This module requires ALBs to be created first by Kubernetes ALB Controller
# Enable this after deploying the Ingress resources
module "waf" {
  count  = var.enable_waf ? 1 : 0
  source = "./modules/waf"

  # Project configuration
  project_name_alias = var.project_name_alias
  workspace          = local.workspace

  # ALB auto-discovery (ALBs created by Kubernetes ALB Controller)
  frontend_alb_name = var.waf_frontend_alb_name
  api_alb_name      = var.waf_api_alb_name

  # Rate limit thresholds (requests per 5-minute window)
  rate_limit_global = var.waf_rate_limit_global
  rate_limit_auth   = var.waf_rate_limit_auth
  rate_limit_chat   = var.waf_rate_limit_chat

  # Tags
  default_tags = local.default_tags
}

# ── CloudFront (Optional, two-stage) ─────────────────────────
# NOTE: Enable after Step 4 — ALB DNS *and* ARN must exist first.
# 1. Deploy Steps 0-4 (creates internal ALBs via K8s Ingress)
# 2. Get ALB info:
#      kubectl get ingress -n ops -o wide
#      aws elbv2 describe-load-balancers --names ops-frontend-alb ops-api-alb \
#        --query 'LoadBalancers[].[LoadBalancerName,LoadBalancerArn,DNSName]' --output table
# 3. Set cloudfront_frontend_alb_arn/_dns and cloudfront_api_alb_arn/_dns in terraform.tfvars
# 4. terraform apply (creates CloudFront distribution + VPC Origins)
module "cloudfront" {
  count = var.enable_cloudfront && var.cloudfront_frontend_alb_arn != "" && var.cloudfront_api_alb_arn != "" ? 1 : 0

  source = "./modules/cloudfront"

  project_name_alias  = var.project_name_alias
  workspace           = local.workspace
  frontend_alb_dns    = var.cloudfront_frontend_alb_dns
  api_alb_dns         = var.cloudfront_api_alb_dns
  frontend_alb_arn    = var.cloudfront_frontend_alb_arn
  api_alb_arn         = var.cloudfront_api_alb_arn
  cf_secret_header    = var.cloudfront_secret_header
  aliases             = var.cloudfront_aliases
  acm_certificate_arn = var.cloudfront_acm_certificate_arn
  default_tags        = local.default_tags
}

# ── Org Cross-Account Setup (optional) ──────────────────────
# Configures trust policies + OpsRole on every child account in the AWS Org.
# Only meaningful when this account is the org management account AND the user
# holds OrganizationAccountAccessRole on each child. For single-account
# deployments (or when the caller lacks org-wide privileges) leave
# `enable_org_cross_account = false` — the default.
module "org_cross_account" {
  count  = var.enable_org_cross_account ? 1 : 0
  source = "./modules/org-cross-account"

  management_account_id = local.account
  default_tags          = local.default_tags
}
