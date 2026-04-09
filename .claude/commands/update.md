# Update OpenOps

Quick application update — rebuild and redeploy without touching infrastructure.

## Default flow

```bash
./deploy-to-existing.sh
```

This does:
1. `build-and-push.sh` — Docker build both images, push to ECR
2. `k8s/deploy.sh init` — Regenerate K8s configs from Terraform outputs
3. `k8s/deploy.sh deploy` — Apply deployments, rolling update

## Selective update

**Backend only:**
```bash
./build-and-push.sh --backend
./deploy-to-existing.sh --skip-build
```

**Frontend only:**
```bash
./build-and-push.sh --frontend
./deploy-to-existing.sh --skip-build
```

**K8s manifests only** (no rebuild, e.g., changed ConfigMap/env vars):
```bash
./deploy-to-existing.sh --skip-build
```

## Verify

```bash
# Watch rollout
kubectl rollout status deployment/openops-backend -n openops
kubectl rollout status deployment/openops-frontend -n openops

# Check pods
kubectl get pods -n openops

# Tail logs
kubectl logs -n openops -l app=openops-backend --tail=100 -f
```

## If something goes wrong

```bash
# Rollback
kubectl rollout undo deployment/openops-backend -n openops
kubectl rollout undo deployment/openops-frontend -n openops

# Check events
kubectl get events -n openops --sort-by='.lastTimestamp' | tail -20
```

$ARGUMENTS
