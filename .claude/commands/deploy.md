# Deploy OpenOps

Full deployment or update of the OpenOps platform to AWS EKS.

## What you have

```
deploy-all.sh           # Full 5-step deployment (step 0-4)
build-and-push.sh       # Docker build + ECR push only
deploy-to-existing.sh   # App-only update (skip infra)
destroy.sh              # Tear down everything
```

## Decision tree

1. **First-time deployment** (no infrastructure exists):
   ```bash
   ./deploy-all.sh --yes
   ```
   This runs all 5 steps: configure → terraform → helm → docker build → k8s deploy.

2. **Code update only** (infrastructure already exists, just pushing new code):
   ```bash
   ./deploy-to-existing.sh
   ```
   Builds Docker images, pushes to ECR, and redeploys to EKS. No Terraform or Helm changes.

3. **Skip Docker build** (images already in ECR, just re-deploy manifests):
   ```bash
   ./deploy-to-existing.sh --skip-build
   ```

4. **Infrastructure change only** (e.g., added a Terraform module):
   ```bash
   ./deploy-all.sh --step 1
   ```

5. **Helm charts only** (e.g., added observability stack):
   ```bash
   ./deploy-all.sh --step 2
   ```

6. **Backend only rebuild**:
   ```bash
   ./build-and-push.sh --backend
   ./deploy-to-existing.sh --skip-build
   ```

7. **Frontend only rebuild**:
   ```bash
   ./build-and-push.sh --frontend
   ./deploy-to-existing.sh --skip-build
   ```

## Steps breakdown

| Step | What | Script |
|------|------|--------|
| 0 | Auto-configure `terraform.tfvars` (account, region, flags) | `deploy-all.sh --step 0` |
| 1 | Terraform apply — VPC, EKS, RDS, Secrets Manager, WAF | `deploy-all.sh --step 1` |
| 2 | Helm install — ALB Controller, Karpenter, Metrics Server, ESO, Redis, Observability | `deploy-all.sh --step 2` |
| 3 | Docker build + ECR push (backend + frontend) | `deploy-all.sh --step 3` |
| 4 | K8s deploy — ConfigMaps, Secrets, Deployments, Services, Ingress | `deploy-all.sh --step 4` |

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
kubectl get pods -n openops
kubectl get ingress -n openops
kubectl logs -n openops -l app=openops-backend --tail=50
```

## Rollback

```bash
# Roll back to previous image
kubectl rollout undo deployment/openops-backend -n openops
kubectl rollout undo deployment/openops-frontend -n openops
```

$ARGUMENTS
