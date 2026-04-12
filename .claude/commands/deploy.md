# Deploy Ops

Full deployment or update of the Ops platform to AWS EKS.

## What you have

```
scripts/deploy-all.sh           # Full 5-step deployment (step 0-4)
scripts/build-and-push.sh       # Docker build + ECR push only
scripts/deploy-to-existing.sh   # App-only update (skip infra)
scripts/destroy.sh              # Tear down everything
```

## Decision tree

1. **First-time deployment** (no infrastructure exists):
   ```bash
   ./scripts/deploy-all.sh --yes
   ```
   This runs all 5 steps: configure → terraform → helm → docker build → k8s deploy.

2. **Code update only** (infrastructure already exists, just pushing new code):
   ```bash
   ./scripts/deploy-to-existing.sh
   ```
   Builds Docker images, pushes to ECR, and redeploys to EKS. No Terraform or Helm changes.

3. **Skip Docker build** (images already in ECR, just re-deploy manifests):
   ```bash
   ./scripts/deploy-to-existing.sh --skip-build
   ```

4. **Infrastructure change only** (e.g., added a Terraform module):
   ```bash
   ./scripts/deploy-all.sh --step 1
   ```

5. **Helm charts only** (e.g., added observability stack):
   ```bash
   ./scripts/deploy-all.sh --step 2
   ```

6. **Backend only rebuild**:
   ```bash
   ./scripts/build-and-push.sh --backend
   ./scripts/deploy-to-existing.sh --skip-build
   ```

7. **Frontend only rebuild**:
   ```bash
   ./scripts/build-and-push.sh --frontend
   ./scripts/deploy-to-existing.sh --skip-build
   ```

## Steps breakdown

| Step | What | Script |
|------|------|--------|
| 0 | Auto-configure `terraform.tfvars` (account, region, flags) | `scripts/deploy-all.sh --step 0` |
| 1 | Terraform apply — VPC, EKS, RDS, Secrets Manager, WAF | `scripts/deploy-all.sh --step 1` |
| 2 | Helm install — ALB Controller, Karpenter, Metrics Server, ESO, Redis, Observability | `scripts/deploy-all.sh --step 2` |
| 3 | Docker build + ECR push (backend + frontend) | `scripts/deploy-all.sh --step 3` |
| 4 | K8s deploy — ConfigMaps, Secrets, Deployments, Services, Ingress | `scripts/deploy-all.sh --step 4` |

## Prerequisites

- AWS CLI configured with appropriate profile (`AWS_PROFILE` or SSO login)
- Terraform, Helm, kubectl, Docker installed
- `iac/terraform.tfvars` exists (Step 0 auto-creates it)

## Environment variables

- `AWS_PROFILE` — AWS CLI profile (auto-detected if not set)
- `AWS_REGION` — Region override
- `SKIP_OBSERVABILITY=true` — Skip Mimir/Loki/Tempo/Alloy in Step 2
- `SKIP_PREDICTION=true` — Disable prediction scheduler in the backend

## After deployment

Verify:
```bash
kubectl get pods -n ops
kubectl get ingress -n ops
kubectl logs -n ops -l app=ops-backend --tail=50
```

## Rollback

```bash
# Roll back to previous image
kubectl rollout undo deployment/ops-backend -n ops
kubectl rollout undo deployment/ops-frontend -n ops
```

$ARGUMENTS
