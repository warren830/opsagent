variable "frontend_domain" {
  description = "Frontend domain (e.g. openops.example.com)"
  type        = string
  default     = ""
}

variable "api_domain" {
  description = "API domain (e.g. api.openops.example.com)"
  type        = string
  default     = ""
}

variable "project_name" {
  description = "The name of the project"
  type        = string
  default     = "openops"
}

variable "project_name_alias" {
  description = "The short name of the project (used in resource naming)"
  type        = string
  default     = "oops"
}

variable "region" {
  description = "AWS region"
  type        = string
  # No default - will be provided via terraform.tfvars, environment variable, or AWS profile
}

variable "account" {
  description = "AWS account"
  type        = string
  # No default - will be provided via terraform.tfvars, environment variable, or detected from AWS profile
}

variable "eks_version" {
  description = "EKS Kubernetes version"
  type        = string
  default     = "1.32"
}

variable "vpc_cidr" {
  description = "CIDR block for VPC"
  type        = string
  default     = "10.10.0.0/22"
}

# WAF configuration
variable "enable_waf" {
  description = "Enable AWS WAF for rate limiting and security protection on ALBs"
  type        = bool
  default     = false
}

variable "waf_frontend_alb_name" {
  description = "Name of the frontend ALB for WAF association (auto-discovery)"
  type        = string
  default     = "openops-frontend-alb"
}

variable "waf_api_alb_name" {
  description = "Name of the API ALB for WAF association (auto-discovery)"
  type        = string
  default     = "openops-api-alb"
}

variable "waf_rate_limit_global" {
  description = "WAF global rate limit per IP (requests per 5 minutes)"
  type        = number
  default     = 2000
}

variable "waf_rate_limit_auth" {
  description = "WAF rate limit per IP for /api/auth/* (requests per 5 minutes)"
  type        = number
  default     = 20
}

variable "waf_rate_limit_chat" {
  description = "WAF rate limit per IP for /api/chat/* (requests per 5 minutes)"
  type        = number
  default     = 300
}
