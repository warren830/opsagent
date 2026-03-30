import { execFile } from 'child_process';
import { loadClusters, ClustersConfig, ClusterInfo, generateClustersKnowledge } from './clusters-loader';
import { loadAccounts, AccountsConfig } from './accounts-loader';
import { discoverClusters } from './cluster-discovery';

function exec(cmd: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, {
      timeout: 30_000,
      maxBuffer: 5 * 1024 * 1024,
    }, (err, stdout, stderr) => {
      if (err) reject(new Error(`${cmd} failed: ${stderr || err.message}`));
      else resolve(stdout.trim());
    });
  });
}

/**
 * 对单个 EKS 集群配置 kubeconfig
 */
async function configureEKSKubeconfig(cluster: ClusterInfo): Promise<void> {
  const roleArn = `arn:aws:iam::${cluster.account}:role/OpsAgentReadOnly`;
  const args = [
    'eks', 'update-kubeconfig',
    '--name', cluster.name,
    '--region', cluster.region,
    '--role-arn', roleArn,
    '--external-id', 'opsagent',
    '--alias', cluster.context,
  ];

  await exec('aws', args);
}

/**
 * 对单个集群配置 kubeconfig（根据云平台分发）
 */
async function configureKubeconfig(cluster: ClusterInfo): Promise<ClusterInfo> {
  try {
    switch (cluster.type) {
      case 'eks':
        await configureEKSKubeconfig(cluster);
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

    // 1. 发现集群
    const discovered = await discoverClusters(clustersConfig, accountsConfig);

    // 2. 并发配置 kubeconfig
    console.log(`[kubeconfig-manager] Configuring kubeconfig for ${discovered.length} clusters...`);
    const configResults = await Promise.allSettled(
      discovered.map(c => configureKubeconfig(c)),
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
