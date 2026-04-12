# Destroy Ops

Tear down all infrastructure. This is destructive and irreversible.

## Before destroying

```bash
# Check what exists
kubectl get all -n ops
cd iac && terraform state list | head -30
```

## Destroy

```bash
./scripts/destroy.sh
```

This removes:
1. K8s application (Deployments, Services, Ingress)
2. Helm releases (ALB Controller, Karpenter, ESO, Redis, Observability)
3. Terraform resources (EKS, RDS, VPC, Secrets Manager, WAF)

## Partial teardown

**App only** (keep infrastructure):
```bash
cd k8s && ./deploy.sh delete
```

**Observability only**:
```bash
helm uninstall alloy -n monitoring
helm uninstall tempo -n monitoring
helm uninstall loki -n monitoring
helm uninstall mimir -n monitoring
```

$ARGUMENTS
