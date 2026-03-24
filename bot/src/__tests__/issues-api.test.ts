import { describe, it } from 'node:test';
import * as assert from 'node:assert/strict';
import { createIssue, issueFingerprint } from '../patrol';
import { createRcaResult, parseConfidence } from '../rca';
import { normalizeAlert } from '../alert-webhook';

describe('Issues API Integration', () => {
  describe('Issue + RCA pipeline', () => {
    it('creates issue then RCA result linked by issue_id', () => {
      const issue = createIssue({
        resource_id: 'i-abc123', resource_type: 'EC2', severity: 'high',
        source: 'patrol', title: 'CPU High', account_name: 'prod',
      });
      issue.id = 42; // simulate DB assignment

      const rca = createRcaResult({
        issue_id: issue.id,
        root_cause: 'Memory leak causing CPU spike',
        confidence: 0.88,
        contributing_factors: ['No memory limits', 'Traffic spike'],
        recommendations: ['Set memory limits', 'Add autoscaling'],
        fix_plan: { steps: ['kubectl set resources ...'] },
        fix_risk_level: 'medium',
        evidence: { metrics: { cpu: 95 } },
        model_id: 'claude-sonnet',
        duration_ms: 12000,
      });

      assert.equal(rca.issue_id, 42);
      assert.equal(rca.confidence, 0.88);
      assert.equal(rca.contributing_factors.length, 2);
    });
  });

  describe('Alert → Issue pipeline', () => {
    it('CloudWatch alert normalizes and creates issue', () => {
      const alert = normalizeAlert({
        Type: 'Notification',
        Message: JSON.stringify({
          AlarmName: 'HighCPU-prod',
          NewStateValue: 'ALARM',
          NewStateReason: 'CPU > 90%',
          Trigger: { MetricName: 'CPUUtilization', Namespace: 'AWS/EC2',
            Dimensions: [{ name: 'InstanceId', value: 'i-prod-1' }] },
        }),
      });
      assert.ok(alert);

      const issue = createIssue({
        resource_id: alert!.resource_id,
        resource_type: alert!.resource_type,
        severity: alert!.severity,
        source: `alert:${alert!.source}`,
        title: alert!.title,
        description: alert!.description,
      });

      assert.equal(issue.resource_id, 'i-prod-1');
      assert.equal(issue.source, 'alert:cloudwatch');
      assert.equal(issue.title, 'HighCPU-prod');
    });
  });

  describe('Issue dedup across sources', () => {
    it('same resource + title + severity produces same fingerprint', () => {
      const fp1 = issueFingerprint('i-123', 'CPU High', 'high');
      const fp2 = issueFingerprint('i-123', 'CPU High', 'high');
      assert.equal(fp1, fp2);
    });

    it('different source does not affect fingerprint', () => {
      // Fingerprint is resource + title + severity only (not source)
      const issue1 = createIssue({ resource_id: 'i-123', severity: 'high', source: 'patrol', title: 'CPU High' });
      const issue2 = createIssue({ resource_id: 'i-123', severity: 'high', source: 'alert:cloudwatch', title: 'CPU High' });
      assert.equal(issue1.fingerprint, issue2.fingerprint);
    });
  });
});
