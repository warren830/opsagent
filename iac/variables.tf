variable "frontend_domain" {
  description = "Frontend domain (e.g. ops.example.com)"
  type        = string
  default     = ""
}

variable "api_domain" {
  description = "API domain (e.g. api.ops.example.com)"
  type        = string
  default     = ""
}

variable "project_name" {
  description = "The name of the project"
  type        = string
  default     = "ops"
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

# Global Accelerator configuration
variable "enable_global_accelerator" {
  description = "Enable AWS Global Accelerator for reduced latency"
  type        = bool
  default     = false
}

variable "ga_frontend_alb_name" {
  description = "Name of the frontend ALB for Global Accelerator (auto-discovery)"
  type        = string
  default     = "ops-frontend-alb"
}

variable "ga_api_alb_name" {
  description = "Name of the API ALB for Global Accelerator (auto-discovery)"
  type        = string
  default     = "ops-api-alb"
}

# Observability configuration
variable "enable_self_hosted_observability" {
  description = "Deploy self-hosted observability stack (Grafana, Mimir, Loki, Tempo) instead of Grafana Cloud"
  type        = bool
  default     = false
}

# Cognito configuration
variable "enable_cognito" {
  description = "Enable AWS Cognito for user authentication (cloud deployments only)"
  type        = bool
  default     = false
}

variable "cognito_callback_urls" {
  description = "List of allowed callback URLs for Cognito OAuth"
  type        = list(string)
  default     = ["http://localhost:3000/auth/cognito/callback"]
}

variable "cognito_logout_urls" {
  description = "List of allowed logout URLs for Cognito"
  type        = list(string)
  default     = ["http://localhost:3000"]
}

variable "cognito_access_token_validity" {
  description = "Cognito access token validity in minutes"
  type        = number
  default     = 60
}

variable "cognito_id_token_validity" {
  description = "Cognito ID token validity in minutes"
  type        = number
  default     = 60
}

variable "cognito_refresh_token_validity" {
  description = "Cognito refresh token validity in days"
  type        = number
  default     = 30
}

variable "cognito_allowed_email_domains" {
  description = "List of allowed email domains for Cognito registration (e.g., ['example.com'])"
  type        = list(string)
  default     = []
}

# CloudFront configuration
variable "enable_cloudfront" {
  description = "Enable CloudFront distribution fronting internal ALBs"
  type        = bool
  default     = false
}

variable "enable_org_cross_account" {
  description = "Bootstrap OpsRole + trust policies on every child AWS Organizations account. Requires this account be the org management account and the caller to hold OrganizationAccountAccessRole on each child. Leave false for single-account deployments."
  type        = bool
  default     = false
}

variable "cloudfront_frontend_alb_dns" {
  description = "DNS name of the frontend ALB (set after K8s Ingress creates it)"
  type        = string
  default     = ""
}

variable "cloudfront_api_alb_dns" {
  description = "DNS name of the API ALB (set after K8s Ingress creates it)"
  type        = string
  default     = ""
}

# CloudFront VPC Origin requires the ALB ARN (not just DNS). Populated after
# k8s creates the ingress — see scripts/deploy-all.sh for the two-stage flow.
variable "cloudfront_frontend_alb_arn" {
  description = "ARN of the frontend internal ALB — used to construct a CloudFront VPC Origin"
  type        = string
  default     = ""
}

variable "cloudfront_api_alb_arn" {
  description = "ARN of the API internal ALB — used to construct a CloudFront VPC Origin"
  type        = string
  default     = ""
}

variable "cloudfront_aliases" {
  description = "Alternate domain names for the CloudFront distribution (e.g. [\"loops.yingchu.cloud\"]). Each alias must be covered by the ACM cert."
  type        = list(string)
  default     = []
}

variable "cloudfront_acm_certificate_arn" {
  description = "ACM certificate ARN to use for CloudFront. Must be in us-east-1 regardless of deployment region. Empty string = default *.cloudfront.net cert."
  type        = string
  default     = ""
}

variable "cloudfront_secret_header" {
  description = "Secret header to verify CloudFront origin requests"
  type        = string
  default     = "ops-cf-secret-2026"
  sensitive   = true
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
  default     = "ops-frontend-alb"
}

variable "waf_api_alb_name" {
  description = "Name of the API ALB for WAF association (auto-discovery)"
  type        = string
  default     = "ops-api-alb"
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
