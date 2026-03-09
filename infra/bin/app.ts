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

const opsAgentStack = new OpsAgentStack(app, 'OpsAgentStack', {
  env: {
    account: hubAccountId || undefined,
    region: process.env.CDK_DEFAULT_REGION || 'us-east-1',
  },
  hubAccountId,
});

new MemberRoleStack(app, 'MemberRoleStack', {
  env: {
    account: hubAccountId || undefined,
    region: process.env.CDK_DEFAULT_REGION || 'us-east-1',
  },
  hubAccountId,
  taskRoleArn: opsAgentStack.taskRoleArn,
  organizationRootOuId: app.node.tryGetContext('organizationRootOuId')
    || process.env.ORGANIZATION_ROOT_OU_ID
    || '',
});
