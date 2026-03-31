import { execFile } from 'child_process';
import { ClustersConfig, ClusterInfo, StaticCluster } from './clusters-loader';
import { AccountsConfig } from './accounts-loader';

interface AccountEntry {
  id: string;
  alias: string;
  roleName: string;
  regions: string[];
}

/**
 * 执行 shell 命令，返回 stdout
 */
function exec(cmd: string, args: string[], env?: Record<string, string>): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, {
      env: { ...process.env, ...env },
      timeout: 30_000,
      maxBuffer: 10 * 1024 * 1024,
    }, (err, stdout, stderr) => {
      if (err) {
        reject(new Error(`${cmd} ${args.slice(0, 3).join(' ')} failed: ${stderr || err.message}`));
      } else {
        resolve(stdout.trim());
      }
    });
  });
}

/**
 * 从 accounts.yaml 构建要扫描的账号列表
 */
function buildAccountList(accountsConfig: AccountsConfig | null): AccountEntry[] {
  if (!accountsConfig?.accounts) return [];
  const { accounts } = accountsConfig;
  const defaultRole = accounts.defaults?.role_name || 'OpsAgentReadOnly';
  const defaultRegions = accounts.defaults?.regions || ['us-east-1'];
  const entries: AccountEntry[] = [];

  // Extra accounts
  for (const extra of accounts.extra || []) {
    entries.push({
      id: extra.id,
      alias: extra.name,
      roleName: extra.role_name || defaultRole,
      regions: extra.regions || defaultRegions,
    });
  }

  return entries;
}

/**
 * 通过 Organizations API 获取所有账号
 */
async function listOrgAccounts(defaultRole: string, defaultRegions: string[], overrides: Record<string, any>): Promise<AccountEntry[]> {
  try {
    const raw = await exec('aws', ['organizations', 'list-accounts', '--output', 'json']);
    const data = JSON.parse(raw);
    const orgAccounts: AccountEntry[] = [];

    for (const acct of data.Accounts || []) {
      if (acct.Status !== 'ACTIVE') continue;
      const id = acct.Id;
      const override = overrides[id];

      // 跳过标记为 skip 的账号
      if (override?.skip) continue;

      orgAccounts.push({
        id,
        alias: override?.alias || acct.Name || id,
        roleName: override?.role_name || defaultRole,
        regions: override?.regions || defaultRegions,
      });
    }
    return orgAccounts;
  } catch (err) {
    console.warn(`[cluster-discovery] Failed to list org accounts: ${(err as Error).message}`);
    return [];
  }
}

/**
 * 对 hub 账号的单个 region 直接发现 EKS 集群（不 assume role，使用当前凭证）
 */
async function discoverEKSInHubRegion(
  accountId: string,
  accountAlias: string,
  region: string,
): Promise<ClusterInfo[]> {
  let clusterNames: string[];
  try {
    const listRaw = await exec('aws', ['eks', 'list-clusters', '--region', region, '--output', 'json']);
    clusterNames = JSON.parse(listRaw).clusters || [];
  } catch (err) {
    console.warn(`[cluster-discovery] list-clusters failed for hub/${region}: ${(err as Error).message}`);
    return [];
  }

  if (clusterNames.length === 0) return [];

  const results: ClusterInfo[] = [];
  for (const name of clusterNames) {
    try {
      const descRaw = await exec('aws', ['eks', 'describe-cluster', '--name', name, '--region', region, '--output', 'json']);
      const cluster = JSON.parse(descRaw).cluster;
      results.push({
        name,
        cloud: 'aws',
        type: 'eks',
        region,
        account: accountId,
        accountAlias,
        version: cluster?.version || 'unknown',
        status: cluster?.status || 'UNKNOWN',
        context: `aws/${accountAlias}/${name}`,
        endpoint: cluster?.endpoint,
      });
    } catch (err) {
      console.warn(`[cluster-discovery] describe-cluster failed for ${name} in hub/${region}: ${(err as Error).message}`);
      results.push({
        name, cloud: 'aws', type: 'eks', region,
        account: accountId, accountAlias,
        version: 'unknown', status: 'UNKNOWN',
        context: `aws/${accountAlias}/${name}`,
      });
    }
  }
  return results;
}

/**
 * 对单个账号的单个 region 发现 EKS 集群
 */
async function discoverEKSInRegion(
  accountId: string,
  accountAlias: string,
  region: string,
  roleName: string,
): Promise<ClusterInfo[]> {
  const roleArn = `arn:aws:iam::${accountId}:role/${roleName}`;

  // 先 assume role 获取临时凭证
  let credentials: { AccessKeyId: string; SecretAccessKey: string; SessionToken: string };
  try {
    const stsRaw = await exec('aws', [
      'sts', 'assume-role',
      '--role-arn', roleArn,
      '--role-session-name', 'opsagent-discovery',
      '--external-id', 'opsagent',
      '--duration-seconds', '900',
      '--output', 'json',
    ]);
    const stsData = JSON.parse(stsRaw);
    credentials = stsData.Credentials;
  } catch (err) {
    console.warn(`[cluster-discovery] assume-role failed for ${accountId} (${roleName}): ${(err as Error).message}`);
    return [];
  }

  const env: Record<string, string> = {
    AWS_ACCESS_KEY_ID: credentials.AccessKeyId,
    AWS_SECRET_ACCESS_KEY: credentials.SecretAccessKey,
    AWS_SESSION_TOKEN: credentials.SessionToken,
    AWS_DEFAULT_REGION: region,
  };

  // list-clusters
  let clusterNames: string[];
  try {
    const listRaw = await exec('aws', ['eks', 'list-clusters', '--region', region, '--output', 'json'], env);
    const listData = JSON.parse(listRaw);
    clusterNames = listData.clusters || [];
  } catch (err) {
    console.warn(`[cluster-discovery] list-clusters failed for ${accountAlias}/${region}: ${(err as Error).message}`);
    return [];
  }

  if (clusterNames.length === 0) return [];

  // describe 每个集群
  const results: ClusterInfo[] = [];
  for (const name of clusterNames) {
    try {
      const descRaw = await exec('aws', [
        'eks', 'describe-cluster', '--name', name, '--region', region, '--output', 'json',
      ], env);
      const descData = JSON.parse(descRaw);
      const cluster = descData.cluster;

      results.push({
        name,
        cloud: 'aws',
        type: 'eks',
        region,
        account: accountId,
        accountAlias,
        version: cluster?.version || 'unknown',
        status: cluster?.status || 'UNKNOWN',
        context: `aws/${accountAlias}/${name}`,
        endpoint: cluster?.endpoint,
      });
    } catch (err) {
      console.warn(`[cluster-discovery] describe-cluster failed for ${name} in ${accountAlias}/${region}: ${(err as Error).message}`);
      // 仍然添加，但版本未知
      results.push({
        name,
        cloud: 'aws',
        type: 'eks',
        region,
        account: accountId,
        accountAlias,
        version: 'unknown',
        status: 'UNKNOWN',
        context: `aws/${accountAlias}/${name}`,
      });
    }
  }

  return results;
}

/**
 * 发现所有 AWS EKS 集群
 */
async function discoverAWS(accountsConfig: AccountsConfig | null): Promise<ClusterInfo[]> {
  const defaults = accountsConfig?.accounts?.defaults;
  const defaultRole = defaults?.role_name || 'OpsAgentReadOnly';
  const defaultRegions = defaults?.regions || ['us-east-1'];
  const overrides = accountsConfig?.accounts?.overrides || {};

  // 获取当前（hub）账号 ID，扫描时跳过，避免向自身 assume-role
  let hubAccountId: string | null = null;
  try {
    const identityRaw = await exec('aws', ['sts', 'get-caller-identity', '--output', 'json']);
    hubAccountId = JSON.parse(identityRaw).Account;
    console.log(`[cluster-discovery] Hub account detected: ${hubAccountId} (will be skipped)`);
  } catch (err) {
    console.warn(`[cluster-discovery] Failed to get caller identity: ${(err as Error).message}`);
  }

  // 从 Organizations 获取账号
  const orgAccounts = await listOrgAccounts(defaultRole, defaultRegions, overrides);

  // 合并 extra 账号
  const extraAccounts = buildAccountList(accountsConfig);

  // 去重（以 account ID 为 key）
  const accountMap = new Map<string, AccountEntry>();
  for (const a of orgAccounts) accountMap.set(a.id, a);
  for (const a of extraAccounts) accountMap.set(a.id, a);  // extra 覆盖 org

  // 从账号列表中移除 hub 账号（hub 账号不需要 assume-role，单独直接扫描）
  if (hubAccountId && accountMap.has(hubAccountId)) {
    console.log(`[cluster-discovery] Removing hub account ${hubAccountId} from assume-role scan list`);
    accountMap.delete(hubAccountId);
  }

  const allAccounts = Array.from(accountMap.values());
  console.log(`[cluster-discovery] Scanning ${allAccounts.length} member accounts + hub account for EKS clusters`);

  // 并发扫描：成员账号（assume role）+ hub 账号（直接用当前凭证）
  const tasks: Promise<ClusterInfo[]>[] = [];

  // Hub 账号：直接扫描，不传 env（使用 OpsAgentTaskRole 当前凭证）
  if (hubAccountId) {
    const hubAlias = overrides[hubAccountId]?.alias || hubAccountId;
    for (const region of defaultRegions) {
      tasks.push(discoverEKSInHubRegion(hubAccountId, hubAlias, region));
    }
  }

  // 成员账号：assume role 扫描
  for (const acct of allAccounts) {
    for (const region of acct.regions) {
      tasks.push(discoverEKSInRegion(acct.id, acct.alias, region, acct.roleName));
    }
  }

  const results = await Promise.allSettled(tasks);
  const clusters: ClusterInfo[] = [];
  for (const r of results) {
    if (r.status === 'fulfilled') {
      clusters.push(...r.value);
    }
  }

  console.log(`[cluster-discovery] Discovered ${clusters.length} EKS clusters`);
  return clusters;
}

/**
 * 将静态集群配置转换为 ClusterInfo
 */
function staticToClusterInfo(s: StaticCluster): ClusterInfo {
  const cloud = (s.cloud || 'aws') as ClusterInfo['cloud'];
  const type = (s.type || 'eks') as ClusterInfo['type'];
  return {
    name: s.name,
    cloud,
    type,
    region: s.region,
    account: s.account_id || '',
    accountAlias: s.account_id || s.name,
    version: 'static',
    status: 'configured',
    context: `${cloud}/${s.account_id || s.name}/${s.name}`,
    description: s.description,
  };
}

/**
 * 主入口：发现所有集群
 */
export async function discoverClusters(
  clustersConfig: ClustersConfig,
  accountsConfig: AccountsConfig | null,
): Promise<ClusterInfo[]> {
  const allClusters: ClusterInfo[] = [];
  const discovery = clustersConfig.clusters?.auto_discovery;

  // AWS EKS
  if (discovery?.aws?.enabled) {
    const awsClusters = await discoverAWS(accountsConfig);
    allClusters.push(...awsClusters);
  }

  // Phase 2+: alicloud, azure, gcp
  // if (discovery?.alicloud?.enabled) { ... }

  // 静态集群
  const staticClusters = clustersConfig.clusters?.static || [];
  for (const s of staticClusters) {
    allClusters.push(staticToClusterInfo(s));
  }

  console.log(`[cluster-discovery] Total clusters: ${allClusters.length} (${allClusters.length - staticClusters.length} discovered + ${staticClusters.length} static)`);
  return allClusters;
}
