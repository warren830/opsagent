variable "project_name_alias" {
  description = "Short project name"
  type        = string
}

variable "workspace" {
  description = "Deployment workspace"
  type        = string
}

variable "frontend_alb_dns" {
  description = "DNS name of the frontend internal ALB"
  type        = string
}

variable "api_alb_dns" {
  description = "DNS name of the API internal ALB"
  type        = string
}

variable "cf_secret_header" {
  description = "Secret header value to verify requests come from CloudFront"
  type        = string
  sensitive   = true
}

variable "default_tags" {
  description = "Default tags"
  type        = map(string)
  default     = {}
}
