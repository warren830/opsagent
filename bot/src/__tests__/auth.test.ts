import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { loadUsers, saveUsers, hashPassword, verifyPassword, parseCookie, SessionStore } from '../auth';

describe('loadUsers', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'auth-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('returns null when file is missing', () => {
    assert.equal(loadUsers(path.join(tmpDir, 'nonexistent.yaml')), null);
  });

  it('parses valid YAML', () => {
    const configPath = path.join(tmpDir, 'users.yaml');
    fs.writeFileSync(configPath, `
users:
  - username: admin
    password_hash: "$2b$10$test"
    role: super_admin
  - username: alpha-ops
    password_hash: "$2b$10$test2"
    role: tenant_admin
    tenant_id: team-alpha
`);
    const result = loadUsers(configPath);
    assert.ok(result);
    assert.equal(result.users.length, 2);
    assert.equal(result.users[0].username, 'admin');
    assert.equal(result.users[0].role, 'super_admin');
    assert.equal(result.users[1].tenant_id, 'team-alpha');
  });

  it('returns null for empty users key', () => {
    const configPath = path.join(tmpDir, 'users.yaml');
    fs.writeFileSync(configPath, 'other_key: true\n');
    assert.equal(loadUsers(configPath), null);
  });
});

describe('saveUsers', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'auth-save-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('writes and reads back correctly', () => {
    const configPath = path.join(tmpDir, 'users.yaml');
    const config = { users: [{ username: 'test', password_hash: '$2b$10$hash', role: 'super_admin' as const }] };
    saveUsers(configPath, config);
    const loaded = loadUsers(configPath);
    assert.ok(loaded);
    assert.equal(loaded.users[0].username, 'test');
  });
});

describe('hashPassword / verifyPassword', () => {
  it('round-trip works', async () => {
    const hash = await hashPassword('mysecret');
    assert.ok(hash.startsWith('$2'));
    assert.ok(await verifyPassword('mysecret', hash));
    assert.equal(await verifyPassword('wrong', hash), false);
  });
});

describe('SessionStore', () => {
  it('create returns UUID-like token', () => {
    const store = new SessionStore();
    const token = store.create({ username: 'admin', role: 'super_admin' });
    assert.ok(token.length > 20);
  });

  it('get returns stored session', () => {
    const store = new SessionStore();
    const token = store.create({ username: 'admin', role: 'super_admin' });
    const session = store.get(token);
    assert.ok(session);
    assert.equal(session.username, 'admin');
    assert.equal(session.role, 'super_admin');
  });

  it('get returns null for unknown token', () => {
    const store = new SessionStore();
    assert.equal(store.get('nonexistent'), null);
  });

  it('destroy removes session', () => {
    const store = new SessionStore();
    const token = store.create({ username: 'admin', role: 'super_admin' });
    store.destroy(token);
    assert.equal(store.get(token), null);
  });

  it('tenant_id is preserved', () => {
    const store = new SessionStore();
    const token = store.create({ username: 'ops', role: 'tenant_admin', tenant_id: 'team-alpha' });
    const session = store.get(token);
    assert.equal(session?.tenant_id, 'team-alpha');
  });
});

describe('parseCookie', () => {
  it('extracts correct value', () => {
    assert.equal(parseCookie('opsagent_session=abc123; other=xyz', 'opsagent_session'), 'abc123');
  });

  it('returns null for missing cookie', () => {
    assert.equal(parseCookie('other=xyz', 'opsagent_session'), null);
  });

  it('returns null for empty header', () => {
    assert.equal(parseCookie('', 'opsagent_session'), null);
  });

  it('handles cookie with no spaces', () => {
    assert.equal(parseCookie('opsagent_session=token123;other=x', 'opsagent_session'), 'token123');
  });
});
