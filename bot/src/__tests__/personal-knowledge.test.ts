import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

import { scanMergedKnowledge } from '../personal-knowledge';

/** Helper: write a minimal .md file with a title and description line. */
function writeMd(dir: string, fileName: string, title: string, desc: string): void {
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, fileName),
    `# ${title}\n\n${desc}\n`,
    'utf-8',
  );
}

describe('scanMergedKnowledge', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'pk-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  // -----------------------------------------------------------
  // Layer 1: Global knowledge only
  // -----------------------------------------------------------
  it('returns global knowledge when no tenantId or username', () => {
    writeMd(tmpDir, 'runbook-db.md', 'DB Runbook', 'How to fix DB issues');
    writeMd(tmpDir, 'runbook-api.md', 'API Runbook', 'API troubleshooting guide');

    const entries = scanMergedKnowledge(tmpDir);

    assert.equal(entries.length, 2);
    const names = entries.map(e => e.fileName);
    assert.ok(names.includes('runbook-api.md'));
    assert.ok(names.includes('runbook-db.md'));
  });

  it('returns empty array when knowledgeDir does not exist', () => {
    const entries = scanMergedKnowledge(path.join(tmpDir, 'nonexistent'));
    assert.equal(entries.length, 0);
  });

  // -----------------------------------------------------------
  // Layer 2: Tenant knowledge overlay
  // -----------------------------------------------------------
  it('merges tenant knowledge over global (union)', () => {
    writeMd(tmpDir, 'runbook-db.md', 'DB Runbook', 'Global DB guide');
    const tenantDir = path.join(tmpDir, '_tenants', 'acme');
    writeMd(tenantDir, 'runbook-acme.md', 'ACME Runbook', 'ACME specific guide');

    const entries = scanMergedKnowledge(tmpDir, 'acme');

    assert.equal(entries.length, 2);
    const names = entries.map(e => e.fileName);
    assert.ok(names.includes('runbook-db.md'));
    assert.ok(names.includes('runbook-acme.md'));
  });

  it('tenant file overrides global file with same fileName', () => {
    writeMd(tmpDir, 'runbook-db.md', 'DB Runbook Global', 'Global version');
    const tenantDir = path.join(tmpDir, '_tenants', 'acme');
    writeMd(tenantDir, 'runbook-db.md', 'DB Runbook Tenant', 'Tenant version');

    const entries = scanMergedKnowledge(tmpDir, 'acme');

    assert.equal(entries.length, 1);
    const db = entries.find(e => e.fileName === 'runbook-db.md')!;
    assert.equal(db.title, 'DB Runbook Tenant');
  });

  it('skips tenant layer when tenantId not provided', () => {
    writeMd(tmpDir, 'runbook-db.md', 'DB Runbook', 'Global');
    const tenantDir = path.join(tmpDir, '_tenants', 'acme');
    writeMd(tenantDir, 'runbook-acme.md', 'ACME Runbook', 'Should not appear');

    const entries = scanMergedKnowledge(tmpDir);

    assert.equal(entries.length, 1);
    assert.equal(entries[0].fileName, 'runbook-db.md');
  });

  it('handles nonexistent tenant directory gracefully', () => {
    writeMd(tmpDir, 'runbook-db.md', 'DB Runbook', 'Global');

    const entries = scanMergedKnowledge(tmpDir, 'nonexistent-tenant');

    assert.equal(entries.length, 1);
    assert.equal(entries[0].fileName, 'runbook-db.md');
  });

  // -----------------------------------------------------------
  // Layer 3: User knowledge overlay
  // -----------------------------------------------------------
  it('merges user knowledge over global (union)', () => {
    writeMd(tmpDir, 'runbook-db.md', 'DB Runbook', 'Global DB guide');
    const userDir = path.join(tmpDir, '_users', 'alice');
    writeMd(userDir, 'runbook-personal.md', 'My Notes', 'Alice personal notes');

    const entries = scanMergedKnowledge(tmpDir, undefined, 'alice');

    assert.equal(entries.length, 2);
    const names = entries.map(e => e.fileName);
    assert.ok(names.includes('runbook-db.md'));
    assert.ok(names.includes('runbook-personal.md'));
  });

  it('user file overrides global file with same fileName', () => {
    writeMd(tmpDir, 'runbook-db.md', 'DB Runbook Global', 'Global version');
    const userDir = path.join(tmpDir, '_users', 'alice');
    writeMd(userDir, 'runbook-db.md', 'DB Runbook User', 'User version');

    const entries = scanMergedKnowledge(tmpDir, undefined, 'alice');

    assert.equal(entries.length, 1);
    const db = entries.find(e => e.fileName === 'runbook-db.md')!;
    assert.equal(db.title, 'DB Runbook User');
  });

  it('auto-creates user directory when it does not exist', () => {
    writeMd(tmpDir, 'runbook-db.md', 'DB Runbook', 'Global');

    const entries = scanMergedKnowledge(tmpDir, undefined, 'newuser');

    // Should return global entries, and the user dir should now exist
    assert.equal(entries.length, 1);
    assert.ok(fs.existsSync(path.join(tmpDir, '_users', 'newuser')));
  });

  // -----------------------------------------------------------
  // Three-layer merge: global + tenant + user
  // -----------------------------------------------------------
  it('merges all three layers with correct priority (user > tenant > global)', () => {
    // Global
    writeMd(tmpDir, 'runbook-db.md', 'DB Global', 'Global version');
    writeMd(tmpDir, 'runbook-api.md', 'API Global', 'Global API guide');
    writeMd(tmpDir, 'runbook-infra.md', 'Infra Global', 'Global infra guide');
    // Tenant overrides runbook-db and adds runbook-acme
    const tenantDir = path.join(tmpDir, '_tenants', 'acme');
    writeMd(tenantDir, 'runbook-db.md', 'DB Tenant', 'Tenant version');
    writeMd(tenantDir, 'runbook-acme.md', 'ACME Runbook', 'ACME specific');
    // User overrides runbook-db (over tenant) and adds runbook-personal
    const userDir = path.join(tmpDir, '_users', 'alice');
    writeMd(userDir, 'runbook-db.md', 'DB User', 'User version');
    writeMd(userDir, 'runbook-personal.md', 'Personal', 'Alice notes');

    const entries = scanMergedKnowledge(tmpDir, 'acme', 'alice');

    // Expected: runbook-acme (tenant), runbook-api (global), runbook-db (user),
    //           runbook-infra (global), runbook-personal (user) — sorted by fileName
    assert.equal(entries.length, 5);

    const byName = new Map(entries.map(e => [e.fileName, e]));
    assert.equal(byName.get('runbook-db.md')!.title, 'DB User');         // user wins
    assert.equal(byName.get('runbook-api.md')!.title, 'API Global');     // global (no override)
    assert.equal(byName.get('runbook-infra.md')!.title, 'Infra Global'); // global (no override)
    assert.equal(byName.get('runbook-acme.md')!.title, 'ACME Runbook');  // tenant unique
    assert.equal(byName.get('runbook-personal.md')!.title, 'Personal');  // user unique
  });

  it('user overrides tenant even when both have same fileName', () => {
    const tenantDir = path.join(tmpDir, '_tenants', 'acme');
    writeMd(tenantDir, 'runbook-db.md', 'DB Tenant', 'Tenant version');
    const userDir = path.join(tmpDir, '_users', 'alice');
    writeMd(userDir, 'runbook-db.md', 'DB User', 'User version');

    const entries = scanMergedKnowledge(tmpDir, 'acme', 'alice');

    const db = entries.find(e => e.fileName === 'runbook-db.md')!;
    assert.equal(db.title, 'DB User');
  });

  // -----------------------------------------------------------
  // relativePath correctness
  // -----------------------------------------------------------
  it('sets correct relativePath for each layer', () => {
    writeMd(tmpDir, 'runbook-global.md', 'Global', 'Global guide');
    const tenantDir = path.join(tmpDir, '_tenants', 'acme');
    writeMd(tenantDir, 'runbook-tenant.md', 'Tenant', 'Tenant guide');
    const userDir = path.join(tmpDir, '_users', 'alice');
    writeMd(userDir, 'runbook-user.md', 'User', 'User guide');

    const entries = scanMergedKnowledge(tmpDir, 'acme', 'alice');

    const byName = new Map(entries.map(e => [e.fileName, e]));
    assert.equal(byName.get('runbook-global.md')!.relativePath, 'knowledge/runbook-global.md');
    assert.equal(byName.get('runbook-tenant.md')!.relativePath, 'knowledge/_tenants/acme/runbook-tenant.md');
    assert.equal(byName.get('runbook-user.md')!.relativePath, 'knowledge/_users/alice/runbook-user.md');
  });

  // -----------------------------------------------------------
  // Sorted output
  // -----------------------------------------------------------
  it('returns entries sorted by fileName', () => {
    writeMd(tmpDir, 'z-runbook.md', 'Z Runbook', 'Z');
    writeMd(tmpDir, 'a-runbook.md', 'A Runbook', 'A');
    writeMd(tmpDir, 'm-runbook.md', 'M Runbook', 'M');

    const entries = scanMergedKnowledge(tmpDir);

    assert.deepEqual(
      entries.map(e => e.fileName),
      ['a-runbook.md', 'm-runbook.md', 'z-runbook.md'],
    );
  });
});
