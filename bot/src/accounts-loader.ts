import * as fs from 'fs';
import * as path from 'path';
import * as yaml from 'js-yaml';

export interface ExtraAccount {
  id: string;
  name: string;
  role_name?: string;
  regions?: string[];
}

export interface AccountOverride {
  alias?: string;
  skip?: boolean;
  role_name?: string;
  regions?: string[];
}

export interface AccountDefaults {
  role_name?: string;
  regions?: string[];
}

export interface CloudAccount {
  name: string;
  region?: string;
  [key: string]: any;  // provider-specific fields
}

export interface AccountsConfig {
  accounts: {
    extra?: ExtraAccount[];
    overrides?: Record<string, AccountOverride>;
    defaults?: AccountDefaults;
    clouds?: Record<string, CloudAccount[]>;
  };
}

export function loadAccounts(configPath: string): AccountsConfig | null {
  const absolutePath = path.resolve(configPath);
  if (!fs.existsSync(absolutePath)) return null;

  try {
    const content = fs.readFileSync(absolutePath, 'utf-8');
    const config = yaml.load(content) as AccountsConfig;
    if (!config || !config.accounts) return null;
    return config;
  } catch (err) {
    console.warn(`[accounts-loader] Failed to load accounts config: ${(err as Error).message}`);
    return null;
  }
}

export function generateAccountsKnowledge(config: AccountsConfig, knowledgeDir: string): string | null {
  const { accounts } = config;
  const mdLines: string[] = ['# Account Directory', ''];

  // AWS Defaults
  if (accounts.defaults) {
    mdLines.push('## AWS Defaults');
    if (accounts.defaults.role_name) mdLines.push(`- Default role: \`${accounts.defaults.role_name}\``);
    if (accounts.defaults.regions?.length) mdLines.push(`- Default regions: ${accounts.defaults.regions.join(', ')}`);
    mdLines.push('');
  }

  // AWS Extra accounts
  const extraAccounts = accounts.extra || [];
  if (extraAccounts.length > 0) {
    mdLines.push('## AWS Extra Accounts');
    mdLines.push('');
    mdLines.push('| Account ID | Name | Role | Regions |');
    mdLines.push('|------------|------|------|---------|');
    for (const acct of extraAccounts) {
      const role = acct.role_name || accounts.defaults?.role_name || '-';
      const regions = acct.regions?.join(', ') || accounts.defaults?.regions?.join(', ') || 'all';
      mdLines.push(`| ${acct.id} | ${acct.name} | ${role} | ${regions} |`);
    }
    mdLines.push('');
  }

  // AWS Overrides
  const overrides = accounts.overrides ? Object.entries(accounts.overrides) : [];
  if (overrides.length > 0) {
    mdLines.push('## AWS Account Overrides');
    mdLines.push('');
    mdLines.push('| Account ID | Alias | Skip | Role Override | Regions Override |');
    mdLines.push('|------------|-------|------|---------------|------------------|');
    for (const [id, override] of overrides) {
      mdLines.push(`| ${id} | ${override.alias || '-'} | ${override.skip ? 'yes' : 'no'} | ${override.role_name || '-'} | ${override.regions?.join(', ') || '-'} |`);
    }
    mdLines.push('');
  }

  // Multi-cloud accounts
  const clouds = accounts.clouds || {};
  for (const [provider, cloudAccounts] of Object.entries(clouds)) {
    if (!cloudAccounts || cloudAccounts.length === 0) continue;
    const providerName = provider.charAt(0).toUpperCase() + provider.slice(1);
    mdLines.push(`## ${providerName} Accounts`);
    mdLines.push('');
    for (const acct of cloudAccounts) {
      mdLines.push(`### ${acct.name}`);
      for (const [k, v] of Object.entries(acct)) {
        if (k === 'name') continue;
        // Don't expose env var names in knowledge file
        if (k.endsWith('_env')) continue;
        mdLines.push(`- ${k}: ${v}`);
      }
      mdLines.push('');
    }
  }

  const hasContent = extraAccounts.length > 0 || overrides.length > 0 || Object.keys(clouds).length > 0;
  if (!hasContent) return null;

  if (!fs.existsSync(knowledgeDir)) {
    fs.mkdirSync(knowledgeDir, { recursive: true });
  }
  fs.writeFileSync(path.join(knowledgeDir, 'accounts.md'), mdLines.join('\n'), 'utf-8');
  console.log(`[accounts-loader] Generated knowledge/accounts.md`);

  // Generate concise summary for system prompt
  const summaryLines: string[] = [];
  for (const acct of extraAccounts) {
    summaryLines.push(`- **${acct.name}** (AWS ${acct.id})`);
  }
  for (const [id, override] of overrides) {
    if (override.alias) summaryLines.push(`- **${override.alias}** (AWS ${id})`);
  }
  for (const [provider, cloudAccounts] of Object.entries(clouds)) {
    for (const acct of cloudAccounts || []) {
      summaryLines.push(`- **${acct.name}** (${provider}${acct.region ? ' ' + acct.region : ''})`);
    }
  }
  return summaryLines.join('\n');
}
