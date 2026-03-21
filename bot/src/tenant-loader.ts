import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

export interface TenantChannel {
  platform: string;      // 'slack' | 'teams' | 'feishu'
  channel_id: string;
}

export interface TenantAlicloudAccount {
  name: string;
  region: string;
  access_key_env: string;
  secret_key_env: string;
}

export interface TenantConfig {
  id: string;
  name: string;
  channels: TenantChannel[];
  aws_account_ids?: string[];
  alicloud?: TenantAlicloudAccount[];
}

export interface TenantsFile {
  tenants: TenantConfig[];
}

export function loadTenants(configPath: string): TenantsFile | null {
  const absolutePath = path.resolve(configPath);
  if (!fs.existsSync(absolutePath)) return null;

  try {
    const content = fs.readFileSync(absolutePath, 'utf-8');
    const config = yaml.load(content) as TenantsFile;
    if (!config || !config.tenants) return null;
    return config;
  } catch (err) {
    console.warn(`[tenant-loader] Failed to load tenants config: ${(err as Error).message}`);
    return null;
  }
}

export function buildChannelIndex(tenants: TenantConfig[]): Map<string, TenantConfig> {
  const index = new Map<string, TenantConfig>();
  for (const tenant of tenants) {
    if (!tenant.channels) continue;
    for (const ch of tenant.channels) {
      const key = `${ch.platform}:${ch.channel_id}`;
      if (index.has(key)) {
        throw new Error(`Duplicate channel mapping: ${key} is assigned to both "${index.get(key)!.id}" and "${tenant.id}"`);
      }
      index.set(key, tenant);
    }
  }
  return index;
}
