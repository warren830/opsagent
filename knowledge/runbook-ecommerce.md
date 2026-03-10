# E-Commerce Platform Runbook

## Architecture Overview

The e-commerce platform runs on EKS cluster `ecommerce-prod` (K8s v1.30) in us-east-1.

### Service Map

```
Internet → CloudFront → ALB (shop-ingress)
                            ├── shop.example.com → web-storefront (shop-frontend)
                            └── m.shop.example.com → mobile-bff (shop-frontend)
                                                        ├── product-catalog (shop-backend)
                                                        ├── shopping-cart (shop-backend)
                                                        ├── order-mgmt (shop-backend)
                                                        └── notification-svc (shop-backend)

payment-gateway (payment) ← isolated PCI-DSS zone
```

### Database Connections

| Service | Database | Host | Type |
|---------|----------|------|------|
| product-catalog | catalog_prod | catalog-prod.cluster-abc123.us-east-1.rds.amazonaws.com | Aurora PostgreSQL |
| order-mgmt | orders_prod | orders-prod.cluster-def456.us-east-1.rds.amazonaws.com | Aurora PostgreSQL |
| shopping-cart | shop-cart-prod | DynamoDB table | DynamoDB |
| web-storefront | - | redis-prod.abc123.ng.0001.use1.cache.amazonaws.com | ElastiCache Redis (session) |

### Key Endpoints

- Production: https://shop.example.com
- Mobile: https://m.shop.example.com
- Staging: https://api-stg.shop.example.com
- CDN: https://d1234abcd.cloudfront.net

## Troubleshooting

### High Latency on Product Pages
1. Check product-catalog pod CPU/memory: `kubectl top pods -n shop-backend -l app=product-catalog`
2. Check Elasticsearch cluster health: check `ELASTICSEARCH_URL` env var
3. Check Redis connection: verify REDIS_URL connectivity
4. Check HPA status: `kubectl get hpa -n shop-frontend storefront-hpa`

### Payment Failures
1. Check payment-gateway logs: `kubectl logs -n payment -l app=payment-gateway`
2. Verify Stripe API key is valid (Secrets Manager: `opsagent/bot-secrets`)
3. Check PCI network policy: `kubectl get networkpolicy -n payment`
4. Verify KMS key access: check ENCRYPTION_KEY_ARN

### Cart Not Persisting
1. Check DynamoDB table `shop-cart-prod` throughput
2. Check shopping-cart pod logs for DynamoDB errors
3. Verify CART_EXPIRY_HOURS setting (currently 72 hours)

## Scaling Policy

| Service | Min | Max | CPU Target | Notes |
|---------|-----|-----|------------|-------|
| web-storefront | 3 | 20 | 60% | Auto-scale during promotions |
| order-mgmt | 2 | 8 | 75% | Scale up during flash sales |
| payment-gateway | 2 | 4 | Manual | PCI zone, manual scaling only |

## On-Call Contacts

- Frontend: @frontend-team (Slack #shop-frontend)
- Backend: @backend-team (Slack #shop-backend)
- Payment: @fintech-team (Slack #payment-oncall) — requires PCI clearance
