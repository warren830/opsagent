import { describe, it } from 'node:test';
import * as assert from 'node:assert/strict';
import { Resource, Issue, RcaResult, ScanLog, query, queryOne, initSchema, isHealthy, close } from '../db';

describe('Database Types', () => {
  it('Resource type has all required fields', () => {
    const r: Resource = {
      id: 1, account_name: 'prod', provider: 'aws', region: 'us-east-1',
      resource_type: 'EC2', resource_id: 'i-123', name: 'web', status: 'running',
      tags: { env: 'prod' }, raw_data: { InstanceType: 't3.micro' }, tenant_id: 'team-alpha',
      first_seen_at: '2026-03-24T00:00:00Z', last_seen_at: '2026-03-24T12:00:00Z',
    };
    assert.equal(r.provider, 'aws');
    assert.equal(r.resource_type, 'EC2');
    assert.equal(r.tags.env, 'prod');
  });

  it('Issue type has all required fields', () => {
    const i: Issue = {
      id: 1, resource_id: 'i-123', resource_type: 'EC2', severity: 'high',
      status: 'open', source: 'patrol', title: 'CPU High',
      description: 'CPU > 90% for 30 min', fingerprint: 'abc123',
      occurrence_count: 3, metric_data: { cpu: 95 }, related_changes: [],
      tenant_id: 'team-alpha', account_name: 'prod',
      created_at: '2026-03-24T00:00:00Z', updated_at: '2026-03-24T00:00:00Z',
      resolved_at: null,
    };
    assert.equal(i.severity, 'high');
    assert.equal(i.occurrence_count, 3);
  });

  it('RcaResult type has all required fields', () => {
    const r: RcaResult = {
      id: 1, issue_id: 1, root_cause: 'Memory leak in worker process',
      confidence: 0.85, contributing_factors: ['High traffic', 'No autoscaling'],
      recommendations: ['Add HPA', 'Increase memory limit'],
      fix_plan: { steps: ['kubectl apply -f hpa.yaml'] },
      fix_risk_level: 'medium', evidence: { metrics: [], logs: [] },
      model_id: 'claude-sonnet', duration_ms: 5000,
      created_at: '2026-03-24T00:00:00Z',
    };
    assert.equal(r.confidence, 0.85);
    assert.equal(r.fix_risk_level, 'medium');
  });

  it('ScanLog type has all required fields', () => {
    const s: ScanLog = {
      id: 1, scan_type: 'full', accounts_scanned: ['prod', 'staging'],
      resources_found: 150, resources_updated: 45, issues_found: 3,
      duration_ms: 12500, errors: [],
      created_at: '2026-03-24T00:00:00Z',
    };
    assert.equal(s.scan_type, 'full');
    assert.equal(s.resources_found, 150);
  });
});

describe('Database Exports', () => {
  it('exports query function', () => {
    assert.equal(typeof query, 'function');
  });

  it('exports queryOne function', () => {
    assert.equal(typeof queryOne, 'function');
  });

  it('exports initSchema function', () => {
    assert.equal(typeof initSchema, 'function');
  });

  it('exports isHealthy function', () => {
    assert.equal(typeof isHealthy, 'function');
  });

  it('exports close function', () => {
    assert.equal(typeof close, 'function');
  });

  it('isHealthy returns false when no DB available', async () => {
    const healthy = await isHealthy();
    assert.equal(healthy, false);
  });
});
