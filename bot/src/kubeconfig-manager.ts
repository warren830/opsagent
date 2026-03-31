import { execFile } from 'child_process';
import { loadClusters, ClustersConfig, ClusterInfo, generateClustersKnowledge } from './clusters-loader';
import { loadAccounts, AccountsConfig } from './accounts-loader';
import { discoverClusters } from './cluster-discovery';

function exec(cmd: string, args: string[], env?: Record<string, string>): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, {
      timeout: 30_000,
      maxBuffer: 5 * 1024 * 1024,
      env: env ? { ...process.env, ...env } : process.env,
    }, (err, stdout, stderr) => {
      if (err) reject(new Error(`${cmd} failed: ${stderr || err.message}`));
      else resolve(stdout.trim());
    });
  });
}

/**
 * 确保当前角色在 EKS 集群的 access entries 中，自动注册（幂等）
 * - hub 集群：注册 OpsAgentTaskRole 本身
 * - 成员集群：注册 OpsAgentReadOnly（使用成员账号凭证）
 */
async function ensureEKSAccessEntry(
  cluster: ClusterInfo,
  principalArn: string,
  env?: Record<string, string>,
): Promise<void> {
  const baseArgs = ['eks', '--region', cluster.region];

  // 检查是否已存在
  try {
    await exec('aws', [
      ...baseArgs, 'describe-access-entry',
      '--cluster-name', cluster.name,
      '--principal-arn', principalArn,
      '--output', 'json',
    ], env);
    return; // 已存在，跳过
  } catch {
    // 不存在，继续创建
  }

  // 创建 access entry
  try {
    await exec('aws', [
      ...baseArgs, 'create-access-entry',
      '--cluster-name', cluster.name,
      '--principal-arn', principalArn,
      '--output', 'json',
    ], env);

    // 关联只读策略
    await exec('aws', [
      ...baseArgs, 'associate-access-policy',
      '--cluster-name', cluster.name,
      '--principal-arn', principalArn,
      '--policy-arn', 'arn:aws:eks::aws:cluster-access-policy/AmazonEKSViewPolicy',
      '--access-scope', 'type=cluster',
      '--output', 'json',
    ], env);

    console.log(`[kubeconfig-manager] Registered access entry for ${principalArn} on ${cluster.name}`);
  } catch (err) {
    console.warn(`[kubeconfig-manager] Failed to register access entry on ${cluster.name}: ${(err as Error).message}`);
  }
}

/**
 * 对单个 EKS 集群配置 kubeconfig，并确保 access entry 已注册
 * - hub 集群：直接用 OpsAgentTaskRole，无需 assume role
 * - 成员集群：assume OpsAgentReadOnly，并用其凭证自注册 access entry
 */
async function configureEKSKubeconfig(cluster: ClusterInfo, hubAccountId: string | null): Promise<void> {
  const isHubCluster = hubAccountId && cluster.account === hubAccountId;

  const kubeconfigArgs = [
    'eks', 'update-kubeconfig',
    '--name', cluster.name,
    '--region', cluster.region,
    '--alias', cluster.context,
  ];

  if (isHubCluster) {
    // Hub 集群：直接用当前凭证，注册 OpsAgentTaskRole 本身
    await exec('aws', kubeconfigArgs);
    const identity = JSON.parse(await exec('aws', ['sts', 'get-caller-identity', '--output', 'json']));
    await ensureEKSAccessEntry(cluster, identity.Arn.replace(/:assumed-role\/([^/]+)\/.*/, ':role/$1'));
  } else {
    // 成员集群：assume OpsAgentReadOnly，用其凭证自注册 access entry
    const roleArn = `arn:aws:iam::${cluster.account}:role/OpsAgentReadOnly`;
    kubeconfigArgs.push('--role-arn', roleArn, '--external-id', 'opsagent');
    await exec('aws', kubeconfigArgs);

    // 用 OpsAgentReadOnly 凭证自注册 access entry
    try {
      const stsRaw = await exec('aws', [
        'sts', 'assume-role',
        '--role-arn', roleArn,
        '--role-session-name', 'opsagent-eks-register',
        '--external-id', 'opsagent',
        '--output', 'json',
      ]);
      const creds = JSON.parse(stsRaw).Credentials;
      const env: Record<string, string> = {
        AWS_ACCESS_KEY_ID: creds.AccessKeyId,
        AWS_SECRET_ACCESS_KEY: creds.SecretAccessKey,
        AWS_SESSION_TOKEN: creds.SessionToken,
      };
      await ensureEKSAccessEntry(cluster, roleArn, env);
    } catch (err) {
      console.warn(`[kubeconfig-manager] Cannot self-register access entry for ${cluster.name}: ${(err as Error).message}`);
    }
  }
}

/**
 * 对单个集群配置 kubeconfig（根据云平台分发）
 */
async function configureKubeconfig(cluster: ClusterInfo, hubAccountId: string | null): Promise<ClusterInfo> {
  try {
    switch (cluster.type) {
      case 'eks':
        await configureEKSKubeconfig(cluster, hubAccountId);
        break;
      // Phase 2+
      // case 'ack': await configureACKKubeconfig(cluster); break;
      // case 'aks': await configureAKSKubeconfig(cluster); break;
      // case 'gke': await configureGKEKubeconfig(cluster); break;
      default:
        cluster.configError = `Unsupported cluster type: ${cluster.type}`;
    }
  } catch (err) {
    cluster.configError = (err as Error).message;
    console.warn(`[kubeconfig-manager] Failed to configure ${cluster.context}: ${cluster.configError}`);
  }
  return cluster;
}

export class KubeconfigManager {
  private clusters: ClusterInfo[] = [];
  private lastRefresh: Date | null = null;
  private refreshTimer: ReturnType<typeof setInterval> | null = null;
  private clustersConfigPath: string;
  private accountsConfigPath: string;
  private knowledgeDir: string;

  constructor(clustersConfigPath: string, accountsConfigPath: string, knowledgeDir: string) {
    this.clustersConfigPath = clustersConfigPath;
    this.accountsConfigPath = accountsConfigPath;
    this.knowledgeDir = knowledgeDir;
  }

  /**
   * 启动时初始化：发现集群 → 配置 kubeconfig → 生成 clusters.md → 启动定时刷新
   */
  async initialize(): Promise<void> {
    const config = loadClusters(this.clustersConfigPath);
    const refresh = config.clusters?.refresh;

    if (refresh?.on_startup !== false) {
      console.log('[kubeconfig-manager] Starting cluster discovery and kubeconfig configuration...');
      await this.refresh();
    }

    // 启动定时刷新
    const intervalMin = refresh?.interval_minutes ?? 60;
    if (intervalMin > 0) {
      this.refreshTimer = setInterval(() => {
        console.log('[kubeconfig-manager] Scheduled refresh triggered');
        this.refresh().catch(err => {
          console.error(`[kubeconfig-manager] Scheduled refresh failed: ${err}`);
        });
      }, intervalMin * 60 * 1000);
      console.log(`[kubeconfig-manager] Scheduled refresh every ${intervalMin} minutes`);
    }
  }

  /**
   * 执行一次完整的发现 + 配置流程
   */
  async refresh(): Promise<ClusterInfo[]> {
    const startTime = Date.now();
    const clustersConfig = loadClusters(this.clustersConfigPath);
    const accountsConfig = loadAccounts(this.accountsConfigPath);

    // 获取 hub 账号 ID，用于判断集群是否属于本账号
    let hubAccountId: string | null = null;
    try {
      const identityRaw = await exec('aws', ['sts', 'get-caller-identity', '--output', 'json']);
      hubAccountId = JSON.parse(identityRaw).Account;
    } catch (err) {
      console.warn(`[kubeconfig-manager] Failed to get caller identity: ${(err as Error).message}`);
    }

    // 1. 发现集群
    const discovered = await discoverClusters(clustersConfig, accountsConfig);

    // 2. 并发配置 kubeconfig
    console.log(`[kubeconfig-manager] Configuring kubeconfig for ${discovered.length} clusters...`);
    const configResults = await Promise.allSettled(
      discovered.map(c => configureKubeconfig(c, hubAccountId)),
    );

    this.clusters = configResults.map(r =>
      r.status === 'fulfilled' ? r.value : { ...discovered[0], configError: 'Configuration failed' },
    ).filter(Boolean);

    // 3. 生成 knowledge/clusters.md
    generateClustersKnowledge(this.clusters, this.knowledgeDir);

    this.lastRefresh = new Date();
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    const active = this.clusters.filter(c => !c.configError).length;
    const failed = this.clusters.filter(c => c.configError).length;
    console.log(`[kubeconfig-manager] Refresh complete in ${elapsed}s: ${active} active, ${failed} failed`);

    return this.clusters;
  }

  getDiscoveredClusters(): ClusterInfo[] {
    return this.clusters;
  }

  getLastRefreshTime(): Date | null {
    return this.lastRefresh;
  }

  getClustersConfig(): ClustersConfig {
    return loadClusters(this.clustersConfigPath);
  }

  destroy(): void {
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = null;
    }
  }
}
