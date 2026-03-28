import * as fs from 'fs';
import * as yaml from 'js-yaml';
import { UserRole } from './auth';

// ── Types ────────────────────────────────────────────────────────

export interface PlatformIds {
  feishu?: string;   // Feishu open_id
  teams?: string;    // Azure AD object ID (aadObjectId)
  slack?: string;    // Slack user ID
}

export interface UserWithPlatformIds {
  username: string;
  password_hash?: string;
  role: UserRole;
  tenant_id?: string;
  platform_ids?: PlatformIds;
}

export interface OAuthUserInfo {
  platform: 'feishu' | 'teams' | 'slack';
  platformId: string;
  name: string;
  email?: string;
  existingUsername?: string; // If binding to existing account
}

// ── OAuth URL builders ──────────────────────────────────────────

export function buildFeishuAuthUrl(appId: string, redirectUri: string, state: string): string {
  const params = new URLSearchParams({
    app_id: appId,
    redirect_uri: redirectUri,
    state,
    response_type: 'code',
  });
  return `https://open.feishu.cn/open-apis/authen/v1/authorize?${params.toString()}`;
}

export function buildTeamsAuthUrl(
  clientId: string,
  redirectUri: string,
  state: string,
  tenantId: string = 'common',
): string {
  const params = new URLSearchParams({
    client_id: clientId,
    redirect_uri: redirectUri,
    state,
    response_type: 'code',
    scope: 'openid profile email User.Read',
  });
  return `https://login.microsoftonline.com/${tenantId}/oauth2/v2.0/authorize?${params.toString()}`;
}

// ── User lookup by platform ID ──────────────────────────────────

export function findUserByPlatformId(
  usersConfigPath: string,
  platform: string,
  platformId: string,
): UserWithPlatformIds | null {
  if (!fs.existsSync(usersConfigPath)) return null;

  try {
    const content = fs.readFileSync(usersConfigPath, 'utf-8');
    const config = yaml.load(content) as { users?: UserWithPlatformIds[] };
    if (!config?.users) return null;

    return config.users.find(u =>
      u.platform_ids && (u.platform_ids as Record<string, string>)[platform] === platformId,
    ) || null;
  } catch {
    return null;
  }
}

// ── User upsert for OAuth login ─────────────────────────────────

export function upsertOAuthUser(
  usersConfigPath: string,
  info: OAuthUserInfo,
): UserWithPlatformIds {
  let config: { users: UserWithPlatformIds[] };

  if (fs.existsSync(usersConfigPath)) {
    try {
      config = yaml.load(fs.readFileSync(usersConfigPath, 'utf-8')) as any;
      if (!config?.users) config = { users: [] };
    } catch {
      config = { users: [] };
    }
  } else {
    config = { users: [] };
  }

  // Check for existing user (by existing username or by platform ID)
  let existing: UserWithPlatformIds | undefined;
  if (info.existingUsername) {
    existing = config.users.find(u => u.username === info.existingUsername);
  }
  if (!existing) {
    existing = config.users.find(u =>
      u.platform_ids && (u.platform_ids as Record<string, string>)[info.platform] === info.platformId,
    );
  }

  if (existing) {
    // Update: add platform ID to existing user
    if (!existing.platform_ids) existing.platform_ids = {};
    (existing.platform_ids as Record<string, string>)[info.platform] = info.platformId;
    writeUsers(usersConfigPath, config);
    return existing;
  }

  // Create new user
  const username = deriveUsername(info, config.users);
  const newUser: UserWithPlatformIds = {
    username,
    role: 'tenant_admin',
    platform_ids: { [info.platform]: info.platformId } as PlatformIds,
  };
  config.users.push(newUser);
  writeUsers(usersConfigPath, config);
  return newUser;
}

// ── Helpers ─────────────────────────────────────────────────────

function deriveUsername(info: OAuthUserInfo, existingUsers: UserWithPlatformIds[]): string {
  // Try email prefix first
  let base = '';
  if (info.email) {
    base = info.email.split('@')[0].toLowerCase().replace(/[^a-z0-9._-]/g, '');
  }
  if (!base || base.length < 2) {
    // Fallback to sanitized name
    base = info.name.toLowerCase().replace(/[^a-z0-9]/g, '-').replace(/-+/g, '-').replace(/^-|-$/g, '');
  }
  if (!base || base.length < 2) {
    base = 'user';
  }

  const taken = new Set(existingUsers.map(u => u.username));
  if (!taken.has(base)) return base;

  // Append number for uniqueness
  for (let i = 2; i < 100; i++) {
    const candidate = `${base}-${i}`;
    if (!taken.has(candidate)) return candidate;
  }
  return `${base}-${Date.now()}`;
}

function writeUsers(configPath: string, config: { users: UserWithPlatformIds[] }): void {
  const dir = require('path').dirname(configPath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  const tmpPath = configPath + '.tmp.' + Date.now();
  fs.writeFileSync(tmpPath, yaml.dump(config, { lineWidth: 120, noRefs: true }), 'utf-8');
  fs.renameSync(tmpPath, configPath);
}
