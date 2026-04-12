# Update Ops

Quick application update — rebuild and redeploy without touching infrastructure.

## Default flow

```bash
./scripts/deploy-to-existing.sh
```

This does:
1. `scripts/build-and-push.sh` — Docker build both images, push to ECR
2. `k8s/deploy.sh init` — Regenerate K8s configs from Terraform outputs
3. `k8s/deploy.sh deploy` — Apply deployments, rolling update

## Selective update

**Backend only:**
```bash
./scripts/build-and-push.sh --backend
./scripts/deploy-to-existing.sh --skip-build
```

**Frontend only:**
```bash
./scripts/build-and-push.sh --frontend
./scripts/deploy-to-existing.sh --skip-build
```

**K8s manifests only** (no rebuild, e.g., changed ConfigMap/env vars):
```bash
./scripts/deploy-to-existing.sh --skip-build
```

## Verify

```bash
# Watch rollout
kubectl rollout status deployment/ops-backend -n ops
kubectl rollout status deployment/ops-frontend -n ops

# Check pods
kubectl get pods -n ops

# Tail logs
kubectl logs -n ops -l app=ops-backend --tail=100 -f
```

## If something goes wrong

```bash
# Rollback
kubectl rollout undo deployment/ops-backend -n ops
kubectl rollout undo deployment/ops-frontend -n ops

# Check events
kubectl get events -n ops --sort-by='.lastTimestamp' | tail -20
```

$ARGUMENTS
