# CloudFront distribution fronting the two internal ALBs via VPC Origin.
#
# Why VPC Origin (vs. custom origin pointing at an ALB DNS)?
#   - ALBs stay `scheme: internal` — no public DNS, no public IP.
#   - CloudFront reaches the ALB over AWS's managed network, not the
#     public internet. The ALB SG can stay locked to the VPC CIDR.
#   - Together: "SG only open to CloudFront" is enforced at the network
#     layer, not just by a secret header.

# ── Frontend VPC Origin ───────────────────────────────────────────────
resource "aws_cloudfront_vpc_origin" "frontend" {
  vpc_origin_endpoint_config {
    name                   = "${var.project_name_alias}-${var.workspace}-frontend-origin"
    arn                    = var.frontend_alb_arn
    http_port              = 80
    https_port             = 443
    origin_protocol_policy = "http-only"

    origin_ssl_protocols {
      items    = ["TLSv1.2"]
      quantity = 1
    }
  }

  tags = var.default_tags
}

# ── API VPC Origin ────────────────────────────────────────────────────
resource "aws_cloudfront_vpc_origin" "api" {
  vpc_origin_endpoint_config {
    name                   = "${var.project_name_alias}-${var.workspace}-api-origin"
    arn                    = var.api_alb_arn
    http_port              = 80
    https_port             = 443
    origin_protocol_policy = "http-only"

    origin_ssl_protocols {
      items    = ["TLSv1.2"]
      quantity = 1
    }
  }

  tags = var.default_tags
}

# ── CloudFront Distribution ───────────────────────────────────────────
resource "aws_cloudfront_distribution" "main" {
  enabled             = true
  is_ipv6_enabled     = true
  comment             = "${var.project_name_alias} - ${var.workspace}"
  default_root_object = ""
  price_class         = "PriceClass_200" # US, Canada, Europe, Asia
  wait_for_deployment = false

  # Custom domains (Alternate Domain Names). Paired with `viewer_certificate`
  # below — ACM cert must cover every alias or the distribution fails to apply.
  aliases = var.aliases

  # Frontend origin (Nuxt SSR, served by internal ALB)
  origin {
    domain_name = var.frontend_alb_dns
    origin_id   = "frontend"

    vpc_origin_config {
      vpc_origin_id            = aws_cloudfront_vpc_origin.frontend.id
      origin_keepalive_timeout = 5
      origin_read_timeout      = 30
    }

    # Defence-in-depth: even though the ALB is unreachable from outside the
    # VPC, we also inject a shared secret so the backend can reject any
    # request that did not traverse CloudFront.
    custom_header {
      name  = "X-CF-Secret"
      value = var.cf_secret_header
    }
  }

  # API origin (Rust backend, served by internal ALB)
  origin {
    domain_name = var.api_alb_dns
    origin_id   = "api"

    vpc_origin_config {
      vpc_origin_id            = aws_cloudfront_vpc_origin.api.id
      origin_keepalive_timeout = 5
      origin_read_timeout      = 30
    }

    custom_header {
      name  = "X-CF-Secret"
      value = var.cf_secret_header
    }
  }

  # Default behavior → frontend (Nuxt SSR — no caching)
  default_cache_behavior {
    allowed_methods  = ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"]
    cached_methods   = ["GET", "HEAD"]
    target_origin_id = "frontend"

    forwarded_values {
      query_string = true
      headers      = ["Host", "Origin", "Authorization", "Accept-Language"]

      cookies {
        forward = "all"
      }
    }

    viewer_protocol_policy = "redirect-to-https"
    min_ttl                = 0
    default_ttl            = 0
    max_ttl                = 0 # No caching — SSR
    compress               = true
  }

  # /api/* → backend (no caching, forward everything)
  ordered_cache_behavior {
    path_pattern     = "/api/*"
    allowed_methods  = ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"]
    cached_methods   = ["GET", "HEAD"]
    target_origin_id = "api"

    forwarded_values {
      query_string = true
      headers      = ["*"]

      cookies {
        forward = "all"
      }
    }

    viewer_protocol_policy = "redirect-to-https"
    min_ttl                = 0
    default_ttl            = 0
    max_ttl                = 0
    compress               = true
  }

  # /health → backend (tiny cache so health pollers don't hammer the origin)
  ordered_cache_behavior {
    path_pattern     = "/health"
    allowed_methods  = ["GET", "HEAD"]
    cached_methods   = ["GET", "HEAD"]
    target_origin_id = "api"

    forwarded_values {
      query_string = false
      cookies {
        forward = "none"
      }
    }

    viewer_protocol_policy = "redirect-to-https"
    min_ttl                = 0
    default_ttl            = 5
    max_ttl                = 10
  }

  restrictions {
    geo_restriction {
      restriction_type = "none"
    }
  }

  # Viewer certificate: use the ACM cert (must be in us-east-1) when a custom
  # domain is wired up; otherwise fall back to the default *.cloudfront.net.
  dynamic "viewer_certificate" {
    for_each = var.acm_certificate_arn != "" ? [1] : []
    content {
      acm_certificate_arn      = var.acm_certificate_arn
      ssl_support_method       = "sni-only"
      minimum_protocol_version = "TLSv1.2_2021"
    }
  }

  dynamic "viewer_certificate" {
    for_each = var.acm_certificate_arn == "" ? [1] : []
    content {
      cloudfront_default_certificate = true
    }
  }

  tags = var.default_tags
}
