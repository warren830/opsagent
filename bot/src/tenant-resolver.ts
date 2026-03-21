import { TenantConfig, loadTenants, buildChannelIndex } from './tenant-loader';

export interface TenantResolution {
  tenant: TenantConfig | null;
  denied: boolean;
  denyReason?: string;
}

export class TenantResolver {
  private configPath: string;
  private channelIndex = new Map<string, TenantConfig>();
  private tenants: TenantConfig[] = [];
  private loaded = false;

  constructor(configPath: string) {
    this.configPath = configPath;
    this.reload();
  }

  reload(): void {
    const file = loadTenants(this.configPath);
    if (!file) {
      this.tenants = [];
      this.channelIndex = new Map();
      this.loaded = false;
      console.warn('[tenant-resolver] tenants.yaml not found or invalid');
      return;
    }
    this.tenants = file.tenants;
    try {
      this.channelIndex = buildChannelIndex(this.tenants);
    } catch (err) {
      console.error(`[tenant-resolver] ${(err as Error).message}`);
      this.channelIndex = new Map();
    }
    this.loaded = true;
    console.log(`[tenant-resolver] Loaded ${this.tenants.length} tenants, ${this.channelIndex.size} channel mappings`);
  }

  resolve(platform: string, conversationId: string): TenantResolution {
    if (!this.loaded) {
      return { tenant: null, denied: true, denyReason: '租户配置不可用，请联系管理员' };
    }

    const key = `${platform}:${conversationId}`;
    const tenant = this.channelIndex.get(key);
    if (!tenant) {
      return { tenant: null, denied: true, denyReason: '此频道未配置 OpsAgent 访问权限' };
    }

    return { tenant, denied: false };
  }

  getTenants(): TenantConfig[] {
    return this.tenants;
  }
}
