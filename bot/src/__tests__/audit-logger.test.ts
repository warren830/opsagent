import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { AuditLogger, AuditEntry } from '../audit-logger';

function captureStdout(fn: () => void): string[] {
  const lines: string[] = [];
  const originalLog = console.log;
  console.log = (...args: any[]) => {
    lines.push(args.map(String).join(' '));
  };
  try {
    fn();
  } finally {
    console.log = originalLog;
  }
  return lines;
}

function makeEntry(overrides: Partial<AuditEntry> = {}): AuditEntry {
  return {
    platform: 'teams',
    userId: 'user-123',
    userName: 'Test User',
    query: 'list all ECS clusters',
    responseLength: 500,
    durationMs: 1234,
    success: true,
    ...overrides,
  };
}

describe('AuditLogger', () => {
  let logger: AuditLogger;

  beforeEach(() => {
    logger = new AuditLogger();
  });

  it('logs valid JSON to stdout', () => {
    const lines = captureStdout(() => logger.log(makeEntry()));
    assert.ok(lines.length >= 1, 'should produce at least one log line');
    // Every line should be valid JSON
    for (const line of lines) {
      assert.doesNotThrow(() => JSON.parse(line), `Line is not valid JSON: ${line}`);
    }
  });

  it('includes all required fields in the audit record', () => {
    const lines = captureStdout(() => logger.log(makeEntry()));
    const audit = JSON.parse(lines[0]);
    assert.ok(audit.timestamp, 'should have timestamp');
    assert.equal(audit.platform, 'teams');
    assert.equal(audit.userId, 'user-123');
    assert.equal(audit.query, 'list all ECS clusters');
    assert.equal(audit.durationMs, 1234);
    assert.equal(audit.success, true);
  });

  it('emits EMF format with _aws field, CloudWatch namespace, and metrics', () => {
    const lines = captureStdout(() => logger.log(makeEntry()));
    assert.ok(lines.length >= 2, 'should produce at least two log lines (audit + EMF)');
    const emf = JSON.parse(lines[1]);
    assert.ok(emf._aws, 'EMF record should have _aws field');
    assert.ok(emf._aws.Timestamp, 'EMF should have Timestamp');
    assert.ok(Array.isArray(emf._aws.CloudWatchMetrics), 'EMF should have CloudWatchMetrics array');
    const cwm = emf._aws.CloudWatchMetrics[0];
    assert.equal(cwm.Namespace, 'OpsAgent');
    const metricNames = cwm.Metrics.map((m: any) => m.Name);
    assert.ok(metricNames.includes('QueryCount'), 'should include QueryCount metric');
    assert.ok(metricNames.includes('QueryDuration'), 'should include QueryDuration metric');
  });

  it('includes QueryErrors metric on failure', () => {
    const lines = captureStdout(() => logger.log(makeEntry({ success: false, error: 'timeout' })));
    const emf = JSON.parse(lines[1]);
    const metricNames = emf._aws.CloudWatchMetrics[0].Metrics.map((m: any) => m.Name);
    assert.ok(metricNames.includes('QueryErrors'), 'should include QueryErrors metric on failure');
  });

  it('truncates long query text to 200 characters', () => {
    const longQuery = 'a'.repeat(500);
    const lines = captureStdout(() => logger.log(makeEntry({ query: longQuery })));
    const audit = JSON.parse(lines[0]);
    assert.equal(audit.query.length, 200, 'query should be truncated to 200 chars');
  });
});
