#!/usr/bin/env node
import 'source-map-support/register';
import * as cdk from 'aws-cdk-lib';
import { OpsAgentStack } from '../lib/ops-agent-stack';
import { MemberRoleStack } from '../lib/member-role-stack';

const app = new cdk.App();

const hubAccountId = app.node.tryGetContext('hubAccountId')
  || process.env.HUB_ACCOUNT_ID
  || '';

if (!hubAccountId) {
  console.warn('WARNING: hubAccountId not set. Pass via -c hubAccountId=<id> or HUB_ACCOUNT_ID env var.');
}

const targetRegion = app.node.tryGetContext('region')
  || process.env.CDK_DEFAULT_REGION
  || 'us-east-1';

const opsAgentStack = new OpsAgentStack(app, 'OpsAgentStack', {
  env: {
    account: hubAccountId || undefined,
    region: targetRegion,
  },
  hubAccountId,
});

// 本地开发者 ARN：从环境变量或 cdk context 读取，逗号分隔
// 例如：DEV_PRINCIPAL_ARNS="arn:aws:iam::612674025488:role/aws-reserved/sso.amazonaws.com/ap-northeast-1/AWSReservedSSO_AdministratorAccess_2f0b065da337be10"
const devPrincipalArns = (
  app.node.tryGetContext('devPrincipalArns')
  || process.env.DEV_PRINCIPAL_ARNS
  || ''
).split(',').map((s: string) => s.trim()).filter(Boolean);

new MemberRoleStack(app, 'MemberRoleStack', {
  env: {
    account: hubAccountId || undefined,
    region: targetRegion,
  },
  hubAccountId,
  taskRoleArn: opsAgentStack.taskRoleArn,
  organizationRootOuId: app.node.tryGetContext('organizationRootOuId')
    || process.env.ORGANIZATION_ROOT_OU_ID
    || '',
  devPrincipalArns,
});
