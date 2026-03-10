# OpsAgent Knowledge Base - AWS Infrastructure Query Guide

## Account Structure

OpsAgent manages multiple AWS accounts across organizations. Account discovery is automatic:

- Organizations and accounts are discovered dynamically via AWS Organizations API
- Each account has an alias (human-readable name) and a 12-digit account ID
- Always include both the account name and account ID in query results

## Common Query Methods

### K8s / EKS Clusters

- Use `aws eks list-clusters --region us-east-1` to enumerate EKS clusters
- Use `aws eks describe-cluster --name <cluster>` for cluster details
- **IMPORTANT**: Before using kubectl, you MUST configure kubeconfig first:
  ```
  aws eks update-kubeconfig --name <cluster-name> --region us-east-1
  ```
- To query multiple clusters, run update-kubeconfig for each cluster (it adds contexts) then use `kubectl --context <context>` or switch with `kubectl config use-context <context>`
- Context naming convention: `arn:aws:eks:us-east-1:034362076319:cluster/<cluster-name>`
- Common checks: node status, pod health, deployment rollout status, HPA metrics
- To query all clusters, first list them, then loop through each one

### ECR (Container Registry)

- Use `aws ecr describe-repositories` to list repositories
- Use `aws ecr list-images --repository-name <repo>` for image inventory
- Use `aws ecr describe-images` for image details (size, tags, push date)
- Check for untagged images and lifecycle policy compliance

### Route53 (DNS)

- Use `aws route53 list-hosted-zones` to enumerate DNS zones
- Use `aws route53 list-resource-record-sets --hosted-zone-id <id>` for records
- Common lookups: A/CNAME records, alias targets, TTL values

### ACM (Certificate Manager)

- Use `aws acm list-certificates` to enumerate certificates
- Use `aws acm describe-certificate --certificate-arn <arn>` for details
- Key fields: domain names, expiry date, validation status, in-use status

### Networking (VPC / Subnets / NAT)

- Use `aws ec2 describe-vpcs` for VPC inventory
- Use `aws ec2 describe-subnets` for subnet details (CIDR, AZ, available IPs)
- Use `aws ec2 describe-nat-gateways` for NAT gateway status
- Use `aws ec2 describe-route-tables` for routing information
- Check VPC peering connections and transit gateway attachments

### EC2 Instances

- Use `aws ec2 describe-instances` for instance inventory
- Key fields: instance ID, type, state, private/public IP, launch time, tags
- Filter by tags (e.g., Name, Environment) for targeted queries
- Check instance status with `aws ec2 describe-instance-status`

### Security Groups

- Use `aws ec2 describe-security-groups` for SG inventory
- Review inbound/outbound rules for compliance
- Flag overly permissive rules (e.g., 0.0.0.0/0 on sensitive ports)
- Cross-reference with ENIs to find attached resources

### Confluence (via MCP Plugin)

- Search Confluence spaces and pages for operational documentation
- Query runbooks, architecture docs, and incident postmortems
- Use Confluence MCP tools when the plugin is enabled

## Output Specification

- Use Markdown tables for structured output
- Always include account name and account ID columns
- Include region when results span multiple regions
- Sort results consistently (by account name, then resource name)
- Use concise but descriptive column headers

### Example Output Format

| Account Name | Account ID | Region | Resource | Status |
|---|---|---|---|---|
| prod-main | 123456789012 | us-east-1 | eks-cluster-01 | ACTIVE |
| staging | 987654321098 | us-west-2 | eks-cluster-02 | ACTIVE |
