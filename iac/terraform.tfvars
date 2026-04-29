account = "034362076319"
region = "us-west-2"
enable_waf = false
enable_global_accelerator = false
enable_cognito = false
enable_self_hosted_observability = false
enable_cloudfront = true
frontend_domain = ""
api_domain = ""
project_name = "ops"
project_name_alias = "ops"

# CloudFront VPC Origin wiring. These are populated *after* the first
# terraform apply creates the VPC/EKS and the subsequent k8s deploy creates
# the internal ALBs. See scripts/deploy-all.sh for the two-stage flow.
cloudfront_frontend_alb_arn = "arn:aws:elasticloadbalancing:us-west-2:034362076319:loadbalancer/app/ops-frontend-alb/3529415e9b77ebff"
cloudfront_api_alb_arn      = "arn:aws:elasticloadbalancing:us-west-2:034362076319:loadbalancer/app/ops-api-alb/54c02621f3fb5706"
cloudfront_frontend_alb_dns = "internal-ops-frontend-alb-358672614.us-west-2.elb.amazonaws.com"
cloudfront_api_alb_dns      = "internal-ops-api-alb-1353793192.us-west-2.elb.amazonaws.com"

# Custom domain — ACM cert is the shared yingchu.cloud wildcard (us-east-1).
cloudfront_aliases             = ["loops.yingchu.cloud"]
cloudfront_acm_certificate_arn = "arn:aws:acm:us-east-1:034362076319:certificate/74d2cea3-a33c-4841-920b-1d878a629c3a"
