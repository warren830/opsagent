import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

export interface StaticCluster {
  name: string;
  cloud: string;
  type: string;
  account_id?: string;
  region: string;
  role_name?: string;
  cluster_id?: string;
  resource_group?: string;
  subscription?: string;
  project?: string;
  zone?: string;
  profile?: string;
  description?: string;
}

export interface AutoDiscoveryConfig {
  aws?: { enabled: boolean };
  alicloud?: { enabled: boolean };
  azure?: { enabled: boolean };
  gcp?: { enabled: boolean };
}

export interface RefreshConfig {
  on_startup?: boolean;
  interval_minutes?: number;
  before_query?: boolean;
}

export interface ClustersConfig {
  clusters: {
    auto_discovery?: AutoDiscoveryConfig;
    static?: StaticCluster[];
    refresh?: RefreshConfig;
  };
}

export interface ClusterInfo {
  name: string;
  cloud: 'aws' | 'alicloud' | 'azure' | 'gcp';
  type: 'eks' | 'ack' | 'aks' | 'gke';
  region: string;
  account: string;
  accountAlias: string;
  version: string;
  status: string;
  context: string;
  endpoint?: string;
  description?: string;
  configError?: string;  // 非空表示 kubeconfig 配置失败
}

const DEFAULT_CONFIG: ClustersConfig = {
  clusters: {
    auto_discovery: { aws: { enabled: true } },
    static: [],
    refresh: { on_startup: true, interval_minutes: 60, before_query: false },
  },
};

export function loadClusters(configPath: string): ClustersConfig {
  const absolutePath = path.resolve(configPath);
  if (!fs.existsSync(absolutePath)) {
    console.warn(`[clusters-loader] Config not found: ${absolutePath}, using defaults`);
    return DEFAULT_CONFIG;
  }

  try {
    const content = fs.readFileSync(absolutePath, 'utf-8');
    const config = yaml.load(content) as ClustersConfig;
    if (!config || !config.clusters) return DEFAULT_CONFIG;
    return config;
  } catch (err) {
    console.warn(`[clusters-loader] Failed to load config: ${(err as Error).message}`);
    return DEFAULT_CONFIG;
  }
}

export function generateClustersKnowledge(clusters: ClusterInfo[], knowledgeDir: string): string | null {
  if (!clusters || clusters.length === 0) {
    // 写一个空的 clusters.md 说明没有发现集群
    const emptyMd = [
      '# Kubernetes Clusters',
      '',
      '> 此文件由系统自动生成，上次更新: ' + new Date().toISOString(),
      '',
      '当前没有发现任何 Kubernetes 集群。',
    ].join('\n');
    if (!fs.existsSync(knowledgeDir)) fs.mkdirSync(knowledgeDir, { recursive: true });
    fs.writeFileSync(path.join(knowledgeDir, 'clusters.md'), emptyMd, 'utf-8');
    return null;
  }

  const activeClusters = clusters.filter(c => !c.configError);
  const failedClusters = clusters.filter(c => c.configError);

  const lines: string[] = [
    '# Kubernetes Clusters',
    '',
    `> 此文件由系统自动生成，上次更新: ${new Date().toISOString()}`,
    '',
    '## 集群总览',
    '',
    '| 集群 | 云平台 | 账号 | Region | 版本 | 状态 | Context |',
    '|------|--------|------|--------|------|------|---------|',
  ];

  for (const c of activeClusters) {
    const cloudLabel = `${c.cloud.toUpperCase()} ${c.type.toUpperCase()}`;
    lines.push(`| ${c.name} | ${cloudLabel} | ${c.accountAlias} (${c.account}) | ${c.region} | ${c.version} | ${c.status} | ${c.context} |`);
  }

  if (failedClusters.length > 0) {
    lines.push('');
    lines.push('## 配置失败的集群');
    lines.push('');
    for (const c of failedClusters) {
      lines.push(`- ${c.name} (${c.account}/${c.region}): ${c.configError}`);
    }
  }

  lines.push('');
  lines.push('## 使用方式');
  lines.push('');
  lines.push('kubeconfig 已预配置，直接使用 context 查询：');
  lines.push('');
  lines.push('```bash');
  lines.push('# 查询特定集群');
  if (activeClusters.length > 0) {
    lines.push(`kubectl --context ${activeClusters[0].context} get pods -A`);
  }
  lines.push('');
  lines.push('# 查询所有集群');
  lines.push('./scripts/kubectl-all.sh get pods -A');
  lines.push('```');

  if (!fs.existsSync(knowledgeDir)) fs.mkdirSync(knowledgeDir, { recursive: true });
  fs.writeFileSync(path.join(knowledgeDir, 'clusters.md'), lines.join('\n'), 'utf-8');
  console.log(`[clusters-loader] Generated knowledge/clusters.md (${activeClusters.length} active, ${failedClusters.length} failed)`);

  // 返回简短摘要用于系统提示词
  const summaryLines = activeClusters.map(c => `- **${c.name}** (${c.cloud} ${c.region}, context: \`${c.context}\`)`);
  return summaryLines.join('\n');
}
