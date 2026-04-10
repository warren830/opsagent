variable "project_name_alias" {
  description = "The short name of the project"
  type        = string
}

variable "account" {
  description = "AWS account"
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

variable "vpc_id" {
  description = "VPC ID where EFS will be created"
  type        = string
}

variable "private_subnet_ids" {
  description = "List of private subnet IDs for EFS mount targets"
  type        = list(string)
}

variable "eks_nodes_security_group_id" {
  description = "Security group ID of EKS nodes from VPC module (allowed to access EFS via NFS)"
  type        = string
}

variable "eks_node_security_group_id" {
  description = "Security group ID created by EKS module for managed node groups"
  type        = string
  default     = ""
}

variable "kms_key_arn" {
  description = "KMS CMK ARN for EFS encryption. If empty, creates a dedicated KMS key."
  type        = string
  default     = ""
}

variable "default_tags" {
  description = "Default tags to apply to resources"
  type        = map(string)
  default     = {}
}
