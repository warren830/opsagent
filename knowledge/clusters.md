# Kubernetes Clusters

> 此文件由系统自动生成，上次更新: 2026-04-01T01:59:17.777Z

## 集群总览

| 集群 | 云平台 | 账号 | Region | 版本 | 状态 | Context |
|------|--------|------|--------|------|------|---------|
| data-platform | AWS EKS | hub-account (034362076319) | us-east-1 | 1.31 | ACTIVE | aws/hub-account/data-platform |
| ecommerce-prod | AWS EKS | hub-account (034362076319) | us-east-1 | 1.30 | ACTIVE | aws/hub-account/ecommerce-prod |
| legacy-migration | AWS EKS | hub-account (034362076319) | us-east-1 | 1.29 | ACTIVE | aws/hub-account/legacy-migration |
| opsagent-test | AWS EKS | hub-account (034362076319) | us-east-1 | 1.31 | ACTIVE | aws/hub-account/opsagent-test |

## 使用方式

kubeconfig 已预配置，直接使用 context 查询：

```bash
# 查询特定集群
kubectl --context aws/hub-account/data-platform get pods -A

# 查询所有集群
./scripts/kubectl-all.sh get pods -A
```