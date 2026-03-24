import { describe, it, beforeEach } from 'node:test';
import * as assert from 'node:assert/strict';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { ApprovalStore } from '../approval-store';

let store: ApprovalStore;
let tmpFile: string;

describe('ApprovalStore', () => {
  beforeEach(() => {
    tmpFile = path.join(os.tmpdir(), `approvals-test-${Date.now()}.json`);
    store = new ApprovalStore(tmpFile);
  });

  it('creates an approval request with auto-incrementing ID', () => {
    const req = store.create({
      command: 'kubectl delete pod nginx',
      requestedBy: 'user1', requestedByName: 'Alice',
      platform: 'slack', channelId: 'C123',
    });
    assert.equal(req.id, '1');
    assert.equal(req.status, 'pending');
    assert.equal(req.command, 'kubectl delete pod nginx');

    const req2 = store.create({
      command: 'kubectl scale deploy nginx --replicas=3',
      requestedBy: 'user2', requestedByName: 'Bob',
      platform: 'feishu', channelId: 'oc_456', tenantId: 'team-alpha',
    });
    assert.equal(req2.id, '2');
    assert.equal(req2.tenantId, 'team-alpha');
  });

  it('lists requests in reverse order (newest first)', () => {
    store.create({ command: 'cmd1', requestedBy: 'u1', requestedByName: 'A', platform: 'slack', channelId: 'C1' });
    store.create({ command: 'cmd2', requestedBy: 'u2', requestedByName: 'B', platform: 'slack', channelId: 'C2' });
    const list = store.list();
    assert.equal(list.length, 2);
    assert.equal(list[0].command, 'cmd2'); // newest first
  });

  it('filters by status', () => {
    store.create({ command: 'cmd1', requestedBy: 'u1', requestedByName: 'A', platform: 'slack', channelId: 'C1' });
    const req2 = store.create({ command: 'cmd2', requestedBy: 'u2', requestedByName: 'B', platform: 'slack', channelId: 'C2' });
    store.approve(req2.id, 'admin');
    assert.equal(store.list({ status: 'pending' }).length, 1);
    assert.equal(store.list({ status: 'approved' }).length, 1);
  });

  it('filters by tenantId', () => {
    store.create({ command: 'cmd1', requestedBy: 'u1', requestedByName: 'A', platform: 'slack', channelId: 'C1', tenantId: 'alpha' });
    store.create({ command: 'cmd2', requestedBy: 'u2', requestedByName: 'B', platform: 'slack', channelId: 'C2', tenantId: 'beta' });
    assert.equal(store.list({ tenantId: 'alpha' }).length, 1);
    assert.equal(store.list({ tenantId: 'beta' }).length, 1);
  });

  it('approves a pending request', () => {
    const req = store.create({ command: 'cmd', requestedBy: 'u1', requestedByName: 'A', platform: 'slack', channelId: 'C1' });
    const approved = store.approve(req.id, 'admin');
    assert.ok(approved);
    assert.equal(approved!.status, 'approved');
    assert.equal(approved!.resolvedBy, 'admin');
    assert.ok(approved!.resolvedAt);
  });

  it('rejects a pending request with reason', () => {
    const req = store.create({ command: 'cmd', requestedBy: 'u1', requestedByName: 'A', platform: 'slack', channelId: 'C1' });
    const rejected = store.reject(req.id, 'admin', 'Too risky');
    assert.ok(rejected);
    assert.equal(rejected!.status, 'rejected');
    assert.equal(rejected!.rejectReason, 'Too risky');
  });

  it('cannot approve/reject non-pending request', () => {
    const req = store.create({ command: 'cmd', requestedBy: 'u1', requestedByName: 'A', platform: 'slack', channelId: 'C1' });
    store.approve(req.id, 'admin');
    assert.equal(store.approve(req.id, 'admin2'), null); // already approved
    assert.equal(store.reject(req.id, 'admin2'), null);
  });

  it('marks executed with result', () => {
    const req = store.create({ command: 'cmd', requestedBy: 'u1', requestedByName: 'A', platform: 'slack', channelId: 'C1' });
    store.approve(req.id, 'admin');
    store.markExecuted(req.id, 'deployment scaled');
    const updated = store.get(req.id);
    assert.equal(updated!.status, 'executed');
    assert.equal(updated!.executionResult, 'deployment scaled');
  });

  it('marks failed with error', () => {
    const req = store.create({ command: 'cmd', requestedBy: 'u1', requestedByName: 'A', platform: 'slack', channelId: 'C1' });
    store.approve(req.id, 'admin');
    store.markFailed(req.id, 'timeout');
    assert.equal(store.get(req.id)!.status, 'failed');
  });

  it('returns pending count', () => {
    store.create({ command: 'cmd1', requestedBy: 'u1', requestedByName: 'A', platform: 'slack', channelId: 'C1', tenantId: 'alpha' });
    store.create({ command: 'cmd2', requestedBy: 'u2', requestedByName: 'B', platform: 'slack', channelId: 'C2', tenantId: 'beta' });
    assert.equal(store.pendingCount(), 2);
    assert.equal(store.pendingCount('alpha'), 1);
    assert.equal(store.pendingCount('beta'), 1);
  });

  it('persists to disk and reloads', () => {
    store.create({ command: 'persistent', requestedBy: 'u1', requestedByName: 'A', platform: 'slack', channelId: 'C1' });
    // Create a new store from same file
    const store2 = new ApprovalStore(tmpFile);
    const list = store2.list();
    assert.equal(list.length, 1);
    assert.equal(list[0].command, 'persistent');
    // Next ID should continue
    const req2 = store2.create({ command: 'cmd2', requestedBy: 'u2', requestedByName: 'B', platform: 'slack', channelId: 'C2' });
    assert.equal(req2.id, '2');
  });

  it('returns null for non-existent ID', () => {
    assert.equal(store.get('999'), null);
    assert.equal(store.approve('999', 'admin'), null);
    assert.equal(store.reject('999', 'admin'), null);
  });
});
