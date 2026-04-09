# Check OpenOps Status

Check the current state of the deployed platform.

## Run these checks

```bash
# 1. Cluster connectivity
kubectl cluster-info

# 2. All pods
kubectl get pods -n openops -o wide

# 3. Services & endpoints
kubectl get svc -n openops

# 4. Ingress (ALB URL)
kubectl get ingress -n openops

# 5. Recent events (errors/warnings)
kubectl get events -n openops --sort-by='.lastTimestamp' --field-selector type!=Normal | tail -20

# 6. Backend health
BACKEND_POD=$(kubectl get pods -n openops -l app=openops-backend -o jsonpath='{.items[0].metadata.name}' 2>/dev/null)
if [[ -n "$BACKEND_POD" ]]; then
  kubectl exec -n openops "$BACKEND_POD" -- curl -s http://localhost:3080/health
fi

# 7. Image versions
kubectl get deployments -n openops -o jsonpath='{range .items[*]}{.metadata.name}: {.spec.template.spec.containers[0].image}{"\n"}{end}'

# 8. Resource usage
kubectl top pods -n openops 2>/dev/null || echo "(metrics-server not available)"
```

## Observability stack (if installed)

```bash
kubectl get pods -n monitoring
kubectl get svc -n monitoring
```

## Terraform state

```bash
cd iac && terraform output
```

$ARGUMENTS
