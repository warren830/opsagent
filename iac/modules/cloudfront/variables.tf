variable "project_name_alias" {
  description = "Short project name"
  type        = string
}

variable "workspace" {
  description = "Deployment workspace"
  type        = string
}

variable "frontend_alb_dns" {
  description = "DNS name of the frontend internal ALB (used as CloudFront origin's domain_name)"
  type        = string
}

variable "api_alb_dns" {
  description = "DNS name of the API internal ALB (used as CloudFront origin's domain_name)"
  type        = string
}

variable "frontend_alb_arn" {
  description = "ARN of the frontend internal ALB — required for the CloudFront VPC Origin"
  type        = string
}

variable "api_alb_arn" {
  description = "ARN of the API internal ALB — required for the CloudFront VPC Origin"
  type        = string
}

variable "cf_secret_header" {
  description = "Secret header value to verify requests come from CloudFront"
  type        = string
  sensitive   = true
}

variable "aliases" {
  description = "Alternate domain names (CNAMEs) for the distribution. Each must be covered by `acm_certificate_arn`."
  type        = list(string)
  default     = []
}

variable "acm_certificate_arn" {
  description = "ACM certificate ARN (must be in us-east-1). Empty string uses the default *.cloudfront.net cert."
  type        = string
  default     = ""
}

variable "default_tags" {
  description = "Default tags"
  type        = map(string)
  default     = {}
}
