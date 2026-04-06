# Domain Outputs
output "frontend_domain" {
  description = "Frontend domain"
  value       = var.frontend_domain
}

output "api_domain" {
  description = "API domain"
  value       = var.api_domain
}

# General Outputs
output "region" {
  description = "AWS region"
  value       = var.region
}

# VPC Outputs
output "vpc_id" {
  description = "ID of the VPC"
  value       = module.vpc.vpc_id
}

output "vpc_cidr_block" {
  description = "CIDR block of the VPC"
  value       = module.vpc.vpc_cidr_block
}

output "private_subnet_ids" {
  description = "List of IDs of private subnets"
  value       = module.vpc.private_subnet_ids
}

output "public_subnet_ids" {
  description = "List of IDs of public subnets"
  value       = module.vpc.public_subnet_ids
}

# EKS Outputs
output "cluster_id" {
  description = "EKS cluster ID"
  value       = module.eks_karpenter.cluster_id
}

output "cluster_arn" {
  description = "EKS cluster ARN"
  value       = module.eks_karpenter.cluster_arn
}

output "cluster_name" {
  description = "EKS cluster name"
  value       = module.eks_karpenter.cluster_name
}

output "cluster_endpoint" {
  description = "Endpoint for EKS control plane"
  value       = module.eks_karpenter.cluster_endpoint
}

output "cluster_version" {
  description = "The Kubernetes version for the EKS cluster"
  value       = module.eks_karpenter.cluster_version
}

output "cluster_security_group_id" {
  description = "Security group ids attached to the cluster control plane"
  value       = module.eks_karpenter.cluster_security_group_id
}

# Database Outputs
output "rds_cluster_endpoint" {
  description = "RDS Aurora cluster endpoint"
  value       = module.rds_aurora_postgresql.cluster_endpoint
  sensitive   = true
}

output "rds_cluster_reader_endpoint" {
  description = "RDS Aurora cluster reader endpoint"
  value       = module.rds_aurora_postgresql.cluster_reader_endpoint
  sensitive   = true
}

output "rds_cluster_database_name" {
  description = "RDS Aurora cluster database name"
  value       = module.rds_aurora_postgresql.cluster_database_name
}

output "rds_cluster_port" {
  description = "RDS Aurora cluster port"
  value       = module.rds_aurora_postgresql.cluster_port
}

output "rds_secret_name" {
  description = "Secrets Manager secret name containing the RDS password"
  value       = module.rds_aurora_postgresql.secret_name
}

# Secrets Manager Outputs
output "backend_secrets_manager_name" {
  description = "Name of the Secrets Manager secret for backend"
  value       = module.eks_addons.backend_secrets_manager_name
}

# Karpenter Outputs
output "karpenter_queue_name" {
  description = "Name of the SQS queue used by Karpenter"
  value       = module.eks_karpenter.karpenter_queue_name
}

output "karpenter_node_iam_role_name" {
  description = "Name of the Karpenter node IAM role"
  value       = module.eks_karpenter.karpenter_node_iam_role_name
}

output "karpenter_service_account" {
  description = "Name of the Karpenter service account"
  value       = module.eks_karpenter.karpenter_service_account
}

# Security Group Outputs
output "eks_nodes_security_group_id" {
  description = "ID of the EKS nodes security group"
  value       = module.vpc.eks_nodes_security_group_id
}

output "rds_security_group_id" {
  description = "ID of the RDS security group"
  value       = module.vpc.rds_security_group_id
}

# WAF Outputs (conditional)
output "waf_enabled" {
  description = "Whether WAF is enabled"
  value       = var.enable_waf
}

output "waf_web_acl_arn" {
  description = "ARN of the WAF WebACL"
  value       = var.enable_waf ? module.waf[0].web_acl_arn : null
}

output "waf_web_acl_id" {
  description = "ID of the WAF WebACL"
  value       = var.enable_waf ? module.waf[0].web_acl_id : null
}
