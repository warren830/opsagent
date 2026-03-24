import { describe, it } from 'node:test';
import * as assert from 'node:assert/strict';
import * as crypto from 'crypto';
import { createIssue, getIssue, listIssues, resolveIssue, issueFingerprint, deduplicateIssue } from '../patrol';

describe('Patrol — Issue Management', () => {
  describe('issueFingerprint', () => {
    it('generates consistent hash from resource_id + title + severity', () => {
      const fp1 = issueFingerprint('i-123', 'CPU High', 'high');
      const fp2 = issueFingerprint('i-123', 'CPU High', 'high');
      assert.equal(fp1, fp2);
      assert.equal(fp1.length, 16); // 16-char hex
    });

    it('different inputs produce different fingerprints', () => {
      const fp1 = issueFingerprint('i-123', 'CPU High', 'high');
      const fp2 = issueFingerprint('i-456', 'CPU High', 'high');
      assert.notEqual(fp1, fp2);
    });
  });

  describe('createIssue', () => {
    it('returns an Issue object with all required fields', () => {
      const issue = createIssue({
        resource_id: 'i-123',
        resource_type: 'EC2',
        severity: 'high',
        source: 'patrol',
        title: 'CPU utilization above 90%',
        description: 'Instance i-123 CPU at 95% for 30 minutes',
        account_name: 'prod',
        tenant_id: 'team-alpha',
        metric_data: { cpu: 95, threshold: 90 },
      });
      assert.equal(issue.resource_id, 'i-123');
      assert.equal(issue.severity, 'high');
      assert.equal(issue.status, 'open');
      assert.equal(issue.source, 'patrol');
      assert.ok(issue.fingerprint);
      assert.equal(issue.occurrence_count, 1);
      assert.ok(issue.created_at);
    });
  });

  describe('deduplicateIssue', () => {
    it('increments occurrence_count for duplicate fingerprint', () => {
      const existing = createIssue({
        resource_id: 'i-123', resource_type: 'EC2', severity: 'high',
        source: 'patrol', title: 'CPU High', account_name: 'prod',
      });
      existing.occurrence_count = 3;

      const updated = deduplicateIssue(existing);
      assert.equal(updated.occurrence_count, 4);
      assert.ok(updated.updated_at);
    });
  });

  describe('resolveIssue', () => {
    it('sets status to resolved and adds resolved_at', () => {
      const issue = createIssue({
        resource_id: 'i-123', resource_type: 'EC2', severity: 'high',
        source: 'patrol', title: 'CPU High', account_name: 'prod',
      });
      const resolved = resolveIssue(issue);
      assert.equal(resolved.status, 'resolved');
      assert.ok(resolved.resolved_at);
    });
  });

  describe('listIssues / getIssue (in-memory)', () => {
    it('listIssues returns empty array initially', () => {
      const issues = listIssues([]);
      assert.deepEqual(issues, []);
    });

    it('getIssue returns null for non-existent ID', () => {
      const issue = getIssue([], 999);
      assert.equal(issue, null);
    });

    it('getIssue finds issue by ID', () => {
      const issue = createIssue({
        resource_id: 'i-123', resource_type: 'EC2', severity: 'high',
        source: 'patrol', title: 'CPU High', account_name: 'prod',
      });
      issue.id = 42;
      const found = getIssue([issue], 42);
      assert.ok(found);
      assert.equal(found!.title, 'CPU High');
    });
  });
});

describe('Patrol — buildPatrolPrompt', () => {
  it('exports buildPatrolPrompt function', () => {
    const { buildPatrolPrompt } = require('../patrol');
    assert.equal(typeof buildPatrolPrompt, 'function');
  });

  it('includes account name and scope in prompt', () => {
    const { buildPatrolPrompt } = require('../patrol');
    const prompt = buildPatrolPrompt({ accountName: 'prod-aws', scope: 'all', regions: ['us-east-1'] });
    assert.ok(prompt.includes('prod-aws'));
    assert.ok(prompt.includes('us-east-1'));
  });

  it('includes security checks when scope is security or all', () => {
    const { buildPatrolPrompt } = require('../patrol');
    const prompt = buildPatrolPrompt({ accountName: 'prod', scope: 'security', regions: [] });
    assert.ok(prompt.includes('GuardDuty') || prompt.includes('安全'));
  });
});
