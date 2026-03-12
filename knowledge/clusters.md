# Kubernetes Clusters

使用 kubectl 前必须先配置 kubeconfig。

## AWS EKS

| Cluster | Account | Region | 用途 | K8s Version |
|---------|---------|--------|------|-------------|
| ecommerce-prod | prod-main (034362076319) | us-east-1 | 电商平台（storefront、cart、order、payment） | v1.30 |
| data-platform | prod-main (034362076319) | us-east-1 | 数据平台（Kafka Connect、Flink、Airflow、ML） | v1.31 |
| legacy-migration | prod-main (034362076319) | us-east-1 | 核心银行系统迁移（dual-write、legacy adapter） | v1.29 |

> **注意**: `legacy-migration` 集群运行 K8s v1.29，即将到达支持终止日期，计划 Q2 2026 升级到 v1.30+。

### 连接方式

```bash
aws eks update-kubeconfig --name <cluster> --region <region>
```

查询多个集群时，逐个配置后用 `--context` 切换：

```bash
# 配置所有集群
aws eks update-kubeconfig --name ecommerce-prod --region us-east-1
aws eks update-kubeconfig --name data-platform --region us-east-1
aws eks update-kubeconfig --name legacy-migration --region us-east-1

# 使用 context 切换
kubectl --context arn:aws:eks:us-east-1:034362076319:cluster/ecommerce-prod get pods -A
kubectl --context arn:aws:eks:us-east-1:034362076319:cluster/data-platform get pods -A
```

### 集群 Namespace 概览

**ecommerce-prod**:
- `shop-frontend` — web-storefront, mobile-bff
- `shop-backend` — product-catalog, shopping-cart, order-mgmt, notification-svc
- `payment` — payment-gateway (PCI-DSS isolated)

**data-platform**:
- `ingestion` — Kafka Connect, Schema Registry, Debezium CDC
- `processing` — Flink jobs, Spark jobs
- `ml-serving` — ML model serving (recommendation, fraud-detection)
- `airflow` — Airflow scheduler, webserver, workers

**legacy-migration**:
- `core-banking` — account-service, transaction-service
- `legacy-adapter` — oracle-db-adapter, mainframe-gateway, legacy-txn-adapter
- `compliance` — AML scanner, KYC verification
