import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

import {
  sanitizeUsername,
  ensureUserDir,
  getUserKnowledgeDir,
  getUserConfigPath,
} from '../user-config-loader';

describe('sanitizeUsername', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ucl-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('allows alphanumeric and hyphens/underscores/dots', () => {
    assert.equal(sanitizeUsername('alice'), 'alice');
    assert.equal(sanitizeUsername('bob-123'), 'bob-123');
    assert.equal(sanitizeUsername('user_name.ext'), 'user_name.ext');
  });

  it('rejects empty username', () => {
    assert.throws(() => sanitizeUsername(''), /Invalid username/);
  });

  it('rejects path traversal characters', () => {
    assert.throws(() => sanitizeUsername('../etc/passwd'), /Invalid username/);
    assert.throws(() => sanitizeUsername('alice/../../root'), /Invalid username/);
  });

  it('rejects special characters', () => {
    assert.throws(() => sanitizeUsername('alice;rm -rf'), /Invalid username/);
    assert.throws(() => sanitizeUsername('bob<script>'), /Invalid username/);
  });

  it('rejects reserved names', () => {
    assert.throws(() => sanitizeUsername('_config'), /Invalid username/);
    assert.throws(() => sanitizeUsername('_tenants'), /Invalid username/);
    assert.throws(() => sanitizeUsername('_users'), /Invalid username/);
  });
});

describe('ensureUserDir', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ucl-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('creates user directory with _config subdirectory', () => {
    const dir = ensureUserDir(tmpDir, 'alice');
    assert.ok(fs.existsSync(dir));
    assert.ok(fs.existsSync(path.join(dir, '_config')));
  });

  it('is idempotent — calling twice does not error', () => {
    ensureUserDir(tmpDir, 'alice');
    const dir = ensureUserDir(tmpDir, 'alice');
    assert.ok(fs.existsSync(dir));
  });

  it('creates directory under _users subdirectory', () => {
    const dir = ensureUserDir(tmpDir, 'bob');
    assert.equal(dir, path.join(tmpDir, '_users', 'bob'));
  });

  it('rejects invalid username', () => {
    assert.throws(() => ensureUserDir(tmpDir, '../root'), /Invalid username/);
  });
});

describe('getUserKnowledgeDir', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ucl-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('returns path to user knowledge directory', () => {
    const dir = getUserKnowledgeDir(tmpDir, 'alice');
    assert.equal(dir, path.join(tmpDir, '_users', 'alice'));
  });

  it('auto-creates directory if it does not exist', () => {
    const dir = getUserKnowledgeDir(tmpDir, 'newuser');
    assert.ok(fs.existsSync(dir));
    assert.ok(fs.existsSync(path.join(dir, '_config')));
  });
});

describe('getUserConfigPath', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ucl-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('returns path to user config file', () => {
    const p = getUserConfigPath(tmpDir, 'alice', 'skills.yaml');
    assert.equal(p, path.join(tmpDir, '_users', 'alice', '_config', 'skills.yaml'));
  });

  it('ensures user directory exists', () => {
    getUserConfigPath(tmpDir, 'newuser', 'scheduled-jobs.yaml');
    assert.ok(fs.existsSync(path.join(tmpDir, '_users', 'newuser', '_config')));
  });

  it('rejects path traversal in config filename', () => {
    assert.throws(
      () => getUserConfigPath(tmpDir, 'alice', '../../../etc/passwd'),
      /Invalid config filename/,
    );
  });
});
