import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { TenantResolver } from '../tenant-resolver';

describe('TenantResolver', () => {
  let tmpDir: string;
  let configPath: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tenant-resolver-test-'));
    configPath = path.join(tmpDir, 'tenants.yaml');
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('denies all when tenants.yaml is missing', () => {
    const resolver = new TenantResolver(path.join(tmpDir, 'nonexistent.yaml'));
    const result = resolver.resolve('feishu', 'oc_xxxx');
    assert.equal(result.denied, true);
    assert.ok(result.denyReason?.includes('租户配置不可用'));
  });

  it('denies for unrecognized channel', () => {
    fs.writeFileSync(configPath, `
tenants:
  - id: team-alpha
    name: Team Alpha
    channels:
      - platform: feishu
        channel_id: oc_1111
`);
    const resolver = new TenantResolver(configPath);
    const result = resolver.resolve('feishu', 'oc_unknown');
    assert.equal(result.denied, true);
    assert.ok(result.denyReason?.includes('未配置'));
  });

  it('returns correct tenant for known channel', () => {
    fs.writeFileSync(configPath, `
tenants:
  - id: team-alpha
    name: Team Alpha
    channels:
      - platform: feishu
        channel_id: oc_1111
  - id: team-beta
    name: Team Beta
    channels:
      - platform: slack
        channel_id: C2222
`);
    const resolver = new TenantResolver(configPath);

    const alpha = resolver.resolve('feishu', 'oc_1111');
    assert.equal(alpha.denied, false);
    assert.equal(alpha.tenant?.id, 'team-alpha');

    const beta = resolver.resolve('slack', 'C2222');
    assert.equal(beta.denied, false);
    assert.equal(beta.tenant?.id, 'team-beta');
  });

  it('reload picks up changes', () => {
    fs.writeFileSync(configPath, `
tenants:
  - id: team-alpha
    name: Team Alpha
    channels:
      - platform: feishu
        channel_id: oc_1111
`);
    const resolver = new TenantResolver(configPath);
    assert.equal(resolver.resolve('feishu', 'oc_1111').denied, false);
    assert.equal(resolver.resolve('slack', 'C_new').denied, true);

    // Update config
    fs.writeFileSync(configPath, `
tenants:
  - id: team-alpha
    name: Team Alpha
    channels:
      - platform: feishu
        channel_id: oc_1111
      - platform: slack
        channel_id: C_new
`);
    resolver.reload();
    assert.equal(resolver.resolve('slack', 'C_new').denied, false);
  });

  it('getTenants returns tenant list', () => {
    fs.writeFileSync(configPath, `
tenants:
  - id: a
    name: A
    channels: []
  - id: b
    name: B
    channels: []
`);
    const resolver = new TenantResolver(configPath);
    assert.equal(resolver.getTenants().length, 2);
  });
});
