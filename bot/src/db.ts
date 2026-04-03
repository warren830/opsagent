/**
 * PostgreSQL database client for OpsAgent.
 * Provides typed queries, connection pooling, schema init, and health checks.
 */
import { Pool, PoolConfig } from 'pg';

// ── Types ─────────────────────────────────────────────────────

export interface Resource {
  id: number;
  account_name: string;
  provider: string;
  region: string | null;
  resource_type: string;
  resource_id: string;
  name: string | null;
  status: string | null;
  tags: Record<string, any>;
  raw_data: Record<string, any>;
  tenant_id: string | null;
  first_seen_at: string;
  last_seen_at: string;
}

export interface Issue {
  id: number;
  resource_id: string | null;
  resource_type: string | null;
  severity: string;
  status: string;
  source: string;
  title: string;
  description: string | null;
  fingerprint: string | null;
  occurrence_count: number;
  metric_data: Record<string, any>;
  related_changes: any[];
  tenant_id: string | null;
  account_name: string | null;
  created_at: string;
  updated_at: string;
  resolved_at: string | null;
  deleted_at?: string | null;
}

export interface RcaResult {
  id: number;
  issue_id: number;
  root_cause: string;
  confidence: number;
  contributing_factors: any[];
  recommendations: any[];
  fix_plan: Record<string, any>;
  fix_risk_level: string | null;
  evidence: Record<string, any>;
  model_id: string | null;
  duration_ms: number | null;
  created_at: string;
}

export interface ScanLog {
  id: number;
  scan_type: string;
  accounts_scanned: any[];
  resources_found: number;
  resources_updated: number;
  issues_found: number;
  duration_ms: number | null;
  errors: any[];
  created_at: string;
}

// ── Schema ────────────────────────────────────────────────────

const SCHEMA_SQL = `
CREATE TABLE IF NOT EXISTS resources (
  id SERIAL PRIMARY KEY,
  account_name VARCHAR(100) NOT NULL,
  provider VARCHAR(20) NOT NULL,
  region VARCHAR(50),
  resource_type VARCHAR(50) NOT NULL,
  resource_id VARCHAR(500) NOT NULL,
  name VARCHAR(200),
  status VARCHAR(50),
  tags JSONB DEFAULT '{}',
  raw_data JSONB DEFAULT '{}',
  tenant_id VARCHAR(50),
  first_seen_at TIMESTAMPTZ DEFAULT NOW(),
  last_seen_at TIMESTAMPTZ DEFAULT NOW(),
  UNIQUE(account_name, provider, resource_id)
);

CREATE TABLE IF NOT EXISTS issues (
  id SERIAL PRIMARY KEY,
  resource_id VARCHAR(500),
  resource_type VARCHAR(50),
  severity VARCHAR(20) NOT NULL,
  status VARCHAR(30) DEFAULT 'open',
  source VARCHAR(50) NOT NULL,
  title VARCHAR(500) NOT NULL,
  description TEXT,
  fingerprint VARCHAR(64),
  occurrence_count INT DEFAULT 1,
  metric_data JSONB DEFAULT '{}',
  related_changes JSONB DEFAULT '[]',
  tenant_id VARCHAR(50),
  account_name VARCHAR(100),
  created_at TIMESTAMPTZ DEFAULT NOW(),
  updated_at TIMESTAMPTZ DEFAULT NOW(),
  resolved_at TIMESTAMPTZ,
  deleted_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS rca_results (
  id SERIAL PRIMARY KEY,
  issue_id INT REFERENCES issues(id),
  root_cause TEXT NOT NULL,
  confidence FLOAT NOT NULL,
  contributing_factors JSONB DEFAULT '[]',
  recommendations JSONB DEFAULT '[]',
  fix_plan JSONB DEFAULT '{}',
  fix_risk_level VARCHAR(20),
  evidence JSONB DEFAULT '{}',
  model_id VARCHAR(100),
  duration_ms INT,
  created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS scan_logs (
  id SERIAL PRIMARY KEY,
  scan_type VARCHAR(20) NOT NULL,
  accounts_scanned JSONB DEFAULT '[]',
  resources_found INT DEFAULT 0,
  resources_updated INT DEFAULT 0,
  issues_found INT DEFAULT 0,
  duration_ms INT,
  errors JSONB DEFAULT '[]',
  created_at TIMESTAMPTZ DEFAULT NOW()
);
`;

// ── Migrations ────────────────────────────────────────────────

const MIGRATIONS_SQL = `
ALTER TABLE issues ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
`;

// ── Pool ──────────────────────────────────────────────────────

const DATABASE_URL = process.env.DATABASE_URL ||
  (process.env.DB_HOST
    ? `postgresql://${process.env.DB_USER}:${process.env.DB_PASSWORD}@${process.env.DB_HOST}:${process.env.DB_PORT || '5432'}/${process.env.DB_NAME || 'opsagent'}`
    : 'postgresql://localhost:5432/opsagent');

const poolConfig: PoolConfig = {
  connectionString: DATABASE_URL,
  max: 5,
  idleTimeoutMillis: 30_000,
  connectionTimeoutMillis: 5_000,
};

let pool: Pool | null = null;

function getPool(): Pool {
  if (!pool) {
    pool = new Pool(poolConfig);
  }
  return pool;
}

// ── Public API ────────────────────────────────────────────────

/**
 * Execute a parameterized SQL query and return typed rows.
 */
export async function query<T = any>(sql: string, params?: any[]): Promise<T[]> {
  const result = await getPool().query(sql, params);
  return result.rows as T[];
}

/**
 * Execute a query and return the first row, or null.
 */
export async function queryOne<T = any>(sql: string, params?: any[]): Promise<T | null> {
  const rows = await query<T>(sql, params);
  return rows.length > 0 ? rows[0] : null;
}

/**
 * Initialize the database schema (CREATE TABLE IF NOT EXISTS) and run migrations.
 */
export async function initSchema(): Promise<void> {
  await getPool().query(SCHEMA_SQL);
  await getPool().query(MIGRATIONS_SQL);
  await cleanupOldIssues();
}

/**
 * Delete issues (and cascade their rca_results) older than 30 days.
 */
export async function cleanupOldIssues(): Promise<void> {
  await getPool().query(`DELETE FROM rca_results WHERE issue_id IN (SELECT id FROM issues WHERE created_at < NOW() - INTERVAL '30 days')`);
  await getPool().query(`DELETE FROM issues WHERE created_at < NOW() - INTERVAL '30 days'`);
}

/**
 * Check if the database connection is healthy.
 */
export async function isHealthy(): Promise<boolean> {
  try {
    await getPool().query('SELECT 1');
    return true;
  } catch {
    return false;
  }
}

/**
 * Close the connection pool.
 */
export async function close(): Promise<void> {
  if (pool) {
    await pool.end();
    pool = null;
  }
}
