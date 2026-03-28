/**
 * Three-layer knowledge merging: global -> tenant -> user.
 * User files override tenant files, tenant files override global files (by fileName).
 */
import * as path from 'path';
import { scanKnowledgeFiles, KnowledgeFileEntry } from './knowledge-loader';
import { getUserKnowledgeDir } from './user-config-loader';

/**
 * Scan and merge knowledge from three layers:
 * 1. Global: knowledgeDir root
 * 2. Tenant: knowledgeDir/_tenants/{tenantId}/
 * 3. User:   knowledgeDir/_users/{username}/
 *
 * Merge rule: union by fileName. User > tenant > global priority.
 * Returns sorted by fileName.
 */
export function scanMergedKnowledge(
  knowledgeDir: string,
  tenantId?: string,
  username?: string,
): KnowledgeFileEntry[] {
  // Layer 1: global knowledge
  const globalEntries = scanKnowledgeFiles(knowledgeDir);
  const merged = new Map<string, KnowledgeFileEntry>();

  for (const entry of globalEntries) {
    merged.set(entry.fileName, entry);
  }

  // Layer 2: tenant knowledge (overrides global)
  if (tenantId) {
    const tenantDir = path.join(knowledgeDir, '_tenants', tenantId);
    const tenantEntries = scanKnowledgeFiles(tenantDir);
    for (const entry of tenantEntries) {
      merged.set(entry.fileName, {
        ...entry,
        relativePath: `knowledge/_tenants/${tenantId}/${entry.fileName}`,
      });
    }
  }

  // Layer 3: user knowledge (overrides tenant and global)
  if (username) {
    // Auto-create user directory if it does not exist
    getUserKnowledgeDir(knowledgeDir, username);
    const userDir = path.join(knowledgeDir, '_users', username);
    const userEntries = scanKnowledgeFiles(userDir);
    for (const entry of userEntries) {
      merged.set(entry.fileName, {
        ...entry,
        relativePath: `knowledge/_users/${username}/${entry.fileName}`,
      });
    }
  }

  // Sort by fileName and return
  return Array.from(merged.values()).sort((a, b) => a.fileName.localeCompare(b.fileName));
}
