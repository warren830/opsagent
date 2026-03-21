# Alicloud Operations Runbook

## aliyun CLI Basics

The `aliyun` CLI is pre-installed. Credentials are injected via environment variables per tenant.

## Common Commands

### ECS (Elastic Compute Service)
```bash
# List all instances
aliyun ecs DescribeInstances --RegionId cn-hangzhou --output cols=InstanceId,InstanceName,Status,PublicIpAddress

# Instance details
aliyun ecs DescribeInstanceAttribute --InstanceId i-xxx

# List across regions
for r in cn-hangzhou cn-shanghai cn-beijing cn-shenzhen; do
  echo "=== $r ==="
  aliyun ecs DescribeInstances --RegionId $r --output cols=InstanceId,InstanceName,Status
done
```

### ACK (Container Service for Kubernetes)
```bash
# List clusters
aliyun cs DescribeClusters

# Get cluster kubeconfig
aliyun cs DescribeClusterUserKubeconfig --ClusterId c-xxx

# Configure kubectl for ACK cluster
aliyun cs DescribeClusterUserKubeconfig --ClusterId c-xxx | jq -r '.config' > /tmp/ack-kubeconfig
export KUBECONFIG=/tmp/ack-kubeconfig
kubectl get nodes
```

### SLB (Server Load Balancer)
```bash
# List load balancers
aliyun slb DescribeLoadBalancers --RegionId cn-hangzhou

# Describe listeners
aliyun slb DescribeLoadBalancerAttribute --LoadBalancerId lb-xxx
```

### RDS (Relational Database Service)
```bash
# List instances
aliyun rds DescribeDBInstances --RegionId cn-hangzhou

# Instance details
aliyun rds DescribeDBInstanceAttribute --DBInstanceId rm-xxx

# Slow query log
aliyun rds DescribeSlowLogs --DBInstanceId rm-xxx --StartTime "2024-01-01T00:00Z" --EndTime "2024-01-02T00:00Z"
```

### OSS (Object Storage Service)
```bash
# List buckets
aliyun oss ls

# List objects in bucket
aliyun oss ls oss://bucket-name/

# Bucket info
aliyun oss stat oss://bucket-name
```

### VPC & Networking
```bash
# List VPCs
aliyun vpc DescribeVpcs --RegionId cn-hangzhou

# List VSwitches
aliyun vpc DescribeVSwitches --RegionId cn-hangzhou --VpcId vpc-xxx

# Security groups
aliyun ecs DescribeSecurityGroups --RegionId cn-hangzhou
aliyun ecs DescribeSecurityGroupAttribute --SecurityGroupId sg-xxx --RegionId cn-hangzhou
```

## Cross-Region Query Pattern

```bash
REGIONS="cn-hangzhou cn-shanghai cn-beijing cn-shenzhen cn-chengdu cn-hongkong"
for r in $REGIONS; do
  echo "=== $r ==="
  aliyun ecs DescribeInstances --RegionId $r --output cols=InstanceId,InstanceName,Status 2>/dev/null || echo "  (no access)"
done
```

## Troubleshooting

### Authentication errors
- Check that ALICLOUD_ACCESS_KEY_ID and ALICLOUD_SECRET_ACCESS_KEY are set
- Verify the key has the required RAM permissions
- `aliyun sts GetCallerIdentity` to check current identity

### ACK kubectl connectivity
1. Get cluster details: `aliyun cs DescribeClusters`
2. Check cluster is in "running" state
3. Download kubeconfig: `aliyun cs DescribeClusterUserKubeconfig --ClusterId c-xxx`
4. Test connectivity: `kubectl --kubeconfig /tmp/ack-kubeconfig cluster-info`

### Common error codes
- `InvalidAccessKeyId.NotFound`: Access key does not exist
- `Forbidden.RAM`: RAM policy does not allow this action
- `InvalidRegionId.NotFound`: Region ID is incorrect
- `Throttling`: API rate limit hit, wait and retry
