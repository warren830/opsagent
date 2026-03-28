import * as fs from 'fs';
import * as path from 'path';

const VALID_USERNAME_RE = /^[a-zA-Z0-9][a-zA-Z0-9._-]{0,62}[a-zA-Z0-9]$/;
const RESERVED_NAMES = new Set(['_config', '_tenants', '_users']);
const VALID_CONFIG_FILENAME_RE = /^[a-zA-Z0-9][a-zA-Z0-9._-]*$/;

/**
 * Validate and return username. Rejects path traversal, special chars, and reserved names.
 */
export function sanitizeUsername(username: string): string {
  if (!username || !VALID_USERNAME_RE.test(username)) {
    throw new Error(`Invalid username: "${username}"`);
  }
  if (RESERVED_NAMES.has(username)) {
    throw new Error(`Invalid username: "${username}" is reserved`);
  }
  return username;
}

/**
 * Ensure user directory exists: knowledge/_users/{username}/_config/
 * Returns the user directory path.
 */
export function ensureUserDir(knowledgeDir: string, username: string): string {
  const safe = sanitizeUsername(username);
  const userDir = path.join(knowledgeDir, '_users', safe);
  const configDir = path.join(userDir, '_config');
  if (!fs.existsSync(configDir)) {
    fs.mkdirSync(configDir, { recursive: true });
  }
  return userDir;
}

/**
 * Get the user's knowledge directory (auto-creates if missing).
 */
export function getUserKnowledgeDir(knowledgeDir: string, username: string): string {
  return ensureUserDir(knowledgeDir, username);
}

/**
 * Get the path to a user config file (e.g., skills.yaml, scheduled-jobs.yaml).
 * Auto-creates the user directory if needed.
 */
export function getUserConfigPath(knowledgeDir: string, username: string, configFile: string): string {
  if (!configFile || !VALID_CONFIG_FILENAME_RE.test(configFile) || configFile.includes('/') || configFile.includes('..')) {
    throw new Error(`Invalid config filename: "${configFile}"`);
  }
  const userDir = ensureUserDir(knowledgeDir, username);
  return path.join(userDir, '_config', configFile);
}
