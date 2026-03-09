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

export interface AccountsConfig {
  accounts: {
    extra?: ExtraAccount[];
    overrides?: Record<string, AccountOverride>;
    defaults?: AccountDefaults;
  };
}

/**
 * Load accounts config from a YAML file.
 * Returns null if the file doesn't exist or is empty.
 */
export function loadAccounts(configPath: string): AccountsConfig | null {
  const absolutePath = path.resolve(configPath);

  if (!fs.existsSync(absolutePath)) {
    return null;
  }

  try {
    const content = fs.readFileSync(absolutePath, 'utf-8');
    const config = yaml.load(content) as AccountsConfig;
    if (!config || !config.accounts) {
      return null;
    }
    return config;
  } catch (err) {
    console.warn(`[accounts-loader] Failed to load accounts config: ${(err as Error).message}`);
    return null;
  }
}

/**
 * Generate accounts knowledge file and return a summary for the system prompt.
 */
export function generateAccountsKnowledge(config: AccountsConfig, knowledgeDir: string): string | null {
  const { accounts } = config;
  const mdLines: string[] = ['# AWS Account Directory', ''];

  // Defaults
  if (accounts.defaults) {
    mdLines.push('## Defaults');
    if (accounts.defaults.role_name) {
      mdLines.push(`- Default role: \`${accounts.defaults.role_name}\``);
    }
    if (accounts.defaults.regions?.length) {
      mdLines.push(`- Default regions: ${accounts.defaults.regions.join(', ')}`);
    }
    mdLines.push('');
  }

  // Extra accounts
  const extraAccounts = accounts.extra || [];
  if (extraAccounts.length > 0) {
    mdLines.push('## Extra Accounts');
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

  // Overrides
  const overrides = accounts.overrides ? Object.entries(accounts.overrides) : [];
  if (overrides.length > 0) {
    mdLines.push('## Account Overrides');
    mdLines.push('');
    mdLines.push('| Account ID | Alias | Skip | Role Override | Regions Override |');
    mdLines.push('|------------|-------|------|---------------|------------------|');
    for (const [id, override] of overrides) {
      mdLines.push(`| ${id} | ${override.alias || '-'} | ${override.skip ? 'yes' : 'no'} | ${override.role_name || '-'} | ${override.regions?.join(', ') || '-'} |`);
    }
    mdLines.push('');
  }

  const hasContent = extraAccounts.length > 0 || overrides.length > 0;
  if (!hasContent) return null;

  if (!fs.existsSync(knowledgeDir)) {
    fs.mkdirSync(knowledgeDir, { recursive: true });
  }
  fs.writeFileSync(path.join(knowledgeDir, 'accounts.md'), mdLines.join('\n'), 'utf-8');
  console.log(`[accounts-loader] Generated knowledge/accounts.md`);

  // Generate concise summary for system prompt
  const summaryLines: string[] = [];
  for (const acct of extraAccounts) {
    summaryLines.push(`- **${acct.name}** (${acct.id})`);
  }
  for (const [id, override] of overrides) {
    if (override.alias) {
      summaryLines.push(`- **${override.alias}** (${id})`);
    }
  }
  return summaryLines.join('\n');
}
