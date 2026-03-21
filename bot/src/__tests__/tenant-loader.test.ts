import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { loadTenants, buildChannelIndex, TenantConfig } from '../tenant-loader';

describe('loadTenants', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'tenant-loader-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('returns null when file is missing', () => {
    const result = loadTenants(path.join(tmpDir, 'nonexistent.yaml'));
    assert.equal(result, null);
  });

  it('parses valid YAML', () => {
    const configPath = path.join(tmpDir, 'tenants.yaml');
    fs.writeFileSync(configPath, `
tenants:
  - id: team-alpha
    name: Team Alpha
    channels:
      - platform: feishu
        channel_id: oc_xxxx
    aws_account_ids:
      - "111111111111"
    alicloud:
      - name: alpha-prod
        region: cn-hangzhou
        access_key_env: ALICLOUD_ALPHA_AK
        secret_key_env: ALICLOUD_ALPHA_SK
`);
    const result = loadTenants(configPath);
    assert.ok(result);
    assert.equal(result.tenants.length, 1);
    assert.equal(result.tenants[0].id, 'team-alpha');
    assert.equal(result.tenants[0].channels.length, 1);
    assert.equal(result.tenants[0].aws_account_ids![0], '111111111111');
    assert.equal(result.tenants[0].alicloud![0].name, 'alpha-prod');
  });

  it('returns null for empty tenants key', () => {
    const configPath = path.join(tmpDir, 'tenants.yaml');
    fs.writeFileSync(configPath, 'other_key: true\n');
    const result = loadTenants(configPath);
    assert.equal(result, null);
  });
});

describe('buildChannelIndex', () => {
  it('builds correct map', () => {
    const tenants: TenantConfig[] = [
      { id: 'a', name: 'A', channels: [{ platform: 'feishu', channel_id: 'oc_1' }] },
      { id: 'b', name: 'B', channels: [{ platform: 'slack', channel_id: 'C123' }] },
    ];
    const index = buildChannelIndex(tenants);
    assert.equal(index.size, 2);
    assert.equal(index.get('feishu:oc_1')?.id, 'a');
    assert.equal(index.get('slack:C123')?.id, 'b');
  });

  it('throws on duplicate channelId', () => {
    const tenants: TenantConfig[] = [
      { id: 'a', name: 'A', channels: [{ platform: 'feishu', channel_id: 'oc_1' }] },
      { id: 'b', name: 'B', channels: [{ platform: 'feishu', channel_id: 'oc_1' }] },
    ];
    assert.throws(() => buildChannelIndex(tenants), /Duplicate channel mapping/);
  });

  it('handles empty channels array', () => {
    const tenants: TenantConfig[] = [
      { id: 'a', name: 'A', channels: [] },
    ];
    const index = buildChannelIndex(tenants);
    assert.equal(index.size, 0);
  });
});
