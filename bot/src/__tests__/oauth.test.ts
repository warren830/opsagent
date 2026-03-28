import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import * as yaml from 'js-yaml';

import {
  buildFeishuAuthUrl,
  buildTeamsAuthUrl,
  findUserByPlatformId,
  upsertOAuthUser,
  UserWithPlatformIds,
} from '../oauth';

describe('buildFeishuAuthUrl', () => {
  it('returns correct Feishu OAuth URL', () => {
    const url = buildFeishuAuthUrl('cli_abc', 'https://example.com/callback', 'random-state');
    assert.ok(url.startsWith('https://open.feishu.cn/open-apis/authen/v1/authorize'));
    assert.ok(url.includes('app_id=cli_abc'));
    assert.ok(url.includes('redirect_uri='));
    assert.ok(url.includes('state=random-state'));
  });
});

describe('buildTeamsAuthUrl', () => {
  it('returns correct Microsoft OAuth URL', () => {
    const url = buildTeamsAuthUrl('client-id-123', 'https://example.com/callback', 'random-state', 'common');
    assert.ok(url.startsWith('https://login.microsoftonline.com/common/oauth2/v2.0/authorize'));
    assert.ok(url.includes('client_id=client-id-123'));
    assert.ok(url.includes('redirect_uri='));
    assert.ok(url.includes('state=random-state'));
    assert.ok(url.includes('scope='));
  });
});

describe('findUserByPlatformId', () => {
  let tmpDir: string;
  let usersPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'oauth-test-'));
    usersPath = path.join(tmpDir, 'users.yaml');
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('finds user by feishu open_id', () => {
    const config = {
      users: [
        { username: 'alice', password_hash: '$2a$...', role: 'super_admin',
          platform_ids: { feishu: 'ou_abc123' } },
        { username: 'bob', password_hash: '$2a$...', role: 'tenant_admin' },
      ],
    };
    fs.writeFileSync(usersPath, yaml.dump(config), 'utf-8');

    const user = findUserByPlatformId(usersPath, 'feishu', 'ou_abc123');
    assert.ok(user);
    assert.equal(user!.username, 'alice');
  });

  it('finds user by teams aadObjectId', () => {
    const config = {
      users: [
        { username: 'alice', password_hash: '$2a$...', role: 'super_admin',
          platform_ids: { teams: 'aad-object-id-xyz' } },
      ],
    };
    fs.writeFileSync(usersPath, yaml.dump(config), 'utf-8');

    const user = findUserByPlatformId(usersPath, 'teams', 'aad-object-id-xyz');
    assert.ok(user);
    assert.equal(user!.username, 'alice');
  });

  it('returns null when no match', () => {
    const config = {
      users: [
        { username: 'alice', password_hash: '$2a$...', role: 'super_admin' },
      ],
    };
    fs.writeFileSync(usersPath, yaml.dump(config), 'utf-8');

    const user = findUserByPlatformId(usersPath, 'feishu', 'ou_unknown');
    assert.equal(user, null);
  });

  it('returns null when users file is missing', () => {
    const user = findUserByPlatformId('/nonexistent/users.yaml', 'feishu', 'ou_abc');
    assert.equal(user, null);
  });
});

describe('upsertOAuthUser', () => {
  let tmpDir: string;
  let usersPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'oauth-test-'));
    usersPath = path.join(tmpDir, 'users.yaml');
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('creates new user when no existing match', () => {
    fs.writeFileSync(usersPath, yaml.dump({ users: [] }), 'utf-8');

    const user = upsertOAuthUser(usersPath, {
      platform: 'feishu',
      platformId: 'ou_new123',
      name: 'New User',
      email: 'newuser@company.com',
    });

    assert.equal(user.username, 'newuser');
    assert.deepEqual(user.platform_ids, { feishu: 'ou_new123' });
    assert.equal(user.role, 'tenant_admin');

    // Verify persisted
    const saved = yaml.load(fs.readFileSync(usersPath, 'utf-8')) as any;
    assert.equal(saved.users.length, 1);
    assert.equal(saved.users[0].username, 'newuser');
  });

  it('updates existing user with new platform_id', () => {
    const config = {
      users: [
        { username: 'alice', password_hash: '$2a$hash', role: 'super_admin',
          platform_ids: { feishu: 'ou_abc123' } },
      ],
    };
    fs.writeFileSync(usersPath, yaml.dump(config), 'utf-8');

    const user = upsertOAuthUser(usersPath, {
      platform: 'teams',
      platformId: 'aad-xyz',
      name: 'Alice',
      email: 'alice@company.com',
      existingUsername: 'alice',
    });

    assert.equal(user.username, 'alice');
    assert.deepEqual(user.platform_ids, { feishu: 'ou_abc123', teams: 'aad-xyz' });

    // Original fields preserved
    const saved = yaml.load(fs.readFileSync(usersPath, 'utf-8')) as any;
    assert.equal(saved.users[0].password_hash, '$2a$hash');
    assert.equal(saved.users[0].role, 'super_admin');
  });

  it('derives username from email prefix', () => {
    fs.writeFileSync(usersPath, yaml.dump({ users: [] }), 'utf-8');

    const user = upsertOAuthUser(usersPath, {
      platform: 'feishu',
      platformId: 'ou_abc',
      name: '陈英初',
      email: 'yingchu.chen@company.com',
    });

    assert.equal(user.username, 'yingchu.chen');
  });

  it('handles duplicate username by appending number', () => {
    const config = {
      users: [
        { username: 'alice', password_hash: '$2a$...', role: 'tenant_admin' },
      ],
    };
    fs.writeFileSync(usersPath, yaml.dump(config), 'utf-8');

    const user = upsertOAuthUser(usersPath, {
      platform: 'feishu',
      platformId: 'ou_alice2',
      name: 'Alice 2',
      email: 'alice@other.com',
    });

    assert.equal(user.username, 'alice-2');
  });
});
