import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import * as http from 'http';
import * as yaml from 'js-yaml';
import * as bcrypt from 'bcryptjs';

// ── Types ────────────────────────────────────────────────────────

export type UserRole = 'super_admin' | 'tenant_admin';

export interface UserRecord {
  username: string;
  password_hash: string;
  role: UserRole;
  tenant_id?: string;
}

export interface UsersConfig {
  users: UserRecord[];
}

export interface SessionData {
  username: string;
  role: UserRole;
  tenant_id?: string;
  createdAt: number;
}

// ── User YAML I/O ────────────────────────────────────────────────

export function loadUsers(configPath: string): UsersConfig | null {
  const absolutePath = path.resolve(configPath);
  if (!fs.existsSync(absolutePath)) return null;

  try {
    const content = fs.readFileSync(absolutePath, 'utf-8');
    const config = yaml.load(content) as UsersConfig;
    if (!config || !config.users) return null;
    return config;
  } catch (err) {
    console.warn(`[auth] Failed to load users config: ${(err as Error).message}`);
    return null;
  }
}

export function saveUsers(configPath: string, config: UsersConfig): void {
  const dir = path.dirname(configPath);
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
  fs.writeFileSync(configPath, yaml.dump(config, { lineWidth: 120, noRefs: true }), 'utf-8');
}

// ── Password Hashing ─────────────────────────────────────────────

export async function hashPassword(plain: string): Promise<string> {
  return bcrypt.hash(plain, 10);
}

export async function verifyPassword(plain: string, hash: string): Promise<boolean> {
  return bcrypt.compare(plain, hash);
}

// ── Session Store ────────────────────────────────────────────────

const SESSION_TTL_MS = 4 * 60 * 60 * 1000; // 4 hours

export class SessionStore {
  private sessions = new Map<string, SessionData>();

  create(data: Omit<SessionData, 'createdAt'>): string {
    const token = crypto.randomUUID();
    this.sessions.set(token, { ...data, createdAt: Date.now() });
    return token;
  }

  get(token: string): SessionData | null {
    const session = this.sessions.get(token);
    if (!session) return null;
    if (Date.now() - session.createdAt > SESSION_TTL_MS) {
      this.sessions.delete(token);
      return null;
    }
    return session;
  }

  destroy(token: string): void {
    this.sessions.delete(token);
  }
}

// ── Cookie Helpers ───────────────────────────────────────────────

const COOKIE_NAME = 'opsagent_session';

export function parseCookie(cookieHeader: string, name: string = COOKIE_NAME): string | null {
  if (!cookieHeader) return null;
  const match = cookieHeader.split(';').find(c => c.trim().startsWith(`${name}=`));
  return match ? match.trim().substring(name.length + 1) : null;
}

export function setSessionCookie(res: http.ServerResponse, token: string): void {
  res.setHeader('Set-Cookie', `${COOKIE_NAME}=${token}; HttpOnly; SameSite=Strict; Path=/admin; Max-Age=14400`);
}

export function clearSessionCookie(res: http.ServerResponse): void {
  res.setHeader('Set-Cookie', `${COOKIE_NAME}=; HttpOnly; SameSite=Strict; Path=/admin; Max-Age=0`);
}
