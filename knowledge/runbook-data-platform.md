# Data Platform Runbook

## Architecture Overview

The data platform runs on EKS cluster `data-platform` (K8s v1.31) in us-east-1.

### Data Flow

```
Source DBs → Debezium CDC → MSK (Kafka) → Kafka Connect → S3 (data-lake-raw-prod)
                                              ↓
                                        Schema Registry
                                              ↓
                                    Flink (real-time) → S3 (data-lake-processed-prod)
                                    Spark (batch)    → S3 (data-lake-processed-prod)
                                              ↓
                                        Glue Catalog → Redshift / Athena
                                              ↓
                                    ML Models (SageMaker) → ml-serving namespace
                                              ↓
                                    Airflow (orchestration)
```

### Key Infrastructure

| Component | AWS Service | Identifier |
|-----------|-------------|------------|
| Kafka | Amazon MSK | b-1.data-kafka.xyz789.c2.kafka.us-east-1.amazonaws.com |
| Raw Data Lake | S3 | data-lake-raw-prod |
| Processed Data Lake | S3 | data-lake-processed-prod |
| Checkpoints | S3 | data-lake-checkpoints |
| Data Catalog | AWS Glue | analytics_prod |
| Data Warehouse | Redshift | data-warehouse-prod.abc123.us-east-1.redshift.amazonaws.com |
| ML Models | S3 | ml-models-prod |
| Airflow DB | Aurora | airflow-db.cluster-ghi789.us-east-1.rds.amazonaws.com |

### ML Models

| Model | Version | Endpoint | Purpose |
|-------|---------|----------|---------|
| recommendation-model | v3.2.0 | recommendation-endpoint-prod | Product recommendations |
| fraud-detection | v1.8.0 | N/A (self-hosted) | Real-time fraud scoring |

- Recommendation model retrains weekly (Sunday 6:00 AM UTC)
- Fraud detection threshold: 0.85 (scores above trigger alert)
- Fraud alerts go to SNS topic: fraud-alerts

## Troubleshooting

### Kafka Connect Lag
1. Check connector status: `kubectl exec -n ingestion -it <kafka-connect-pod> -- curl localhost:8083/connectors`
2. Check S3 sink lag: compare S3 object timestamps with Kafka offsets
3. Check Schema Registry health: `kubectl logs -n ingestion -l app=schema-registry`

### Flink Job Failures
1. Check Flink UI: port-forward to flink-jobmanager:8081
2. Check checkpoint failures: verify S3 access to `data-lake-checkpoints`
3. Check Kinesis source: verify clickstream-prod stream is active
4. TaskManager memory: 4GB per TM, 4 slots each

### Airflow DAG Failures
1. Check scheduler logs: `kubectl logs -n airflow -l app=airflow-scheduler`
2. Check Airflow webserver: port-forward to airflow-webserver:8080
3. Check Celery broker (Redis) connectivity
4. Check remote log storage: S3 bucket `airflow-logs-prod`

## Daily Jobs

| Job | Schedule | Namespace | Description |
|-----|----------|-----------|-------------|
| daily-etl-aggregate | 03:30 UTC | processing | Aggregate raw → processed |
| model-retrain-weekly | Sunday 06:00 UTC | ml-serving | Retrain recommendation model |
