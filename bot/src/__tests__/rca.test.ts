import { describe, it } from 'node:test';
import * as assert from 'node:assert/strict';
import { buildRcaPrompt, createRcaResult, parseConfidence, RcaInput } from '../rca';

describe('RCA Engine', () => {
  describe('buildRcaPrompt', () => {
    it('includes issue title and resource_id', () => {
      const prompt = buildRcaPrompt({
        issueId: 42,
        title: 'CPU utilization above 90%',
        resource_id: 'i-abc123',
        resource_type: 'EC2',
        severity: 'high',
        description: 'Sustained high CPU for 30 minutes',
        account_name: 'prod-aws',
        regions: ['us-east-1'],
      });
      assert.ok(prompt.includes('CPU utilization above 90%'));
      assert.ok(prompt.includes('i-abc123'));
      assert.ok(prompt.includes('EC2'));
      assert.ok(prompt.includes('prod-aws'));
    });

    it('includes investigation steps', () => {
      const prompt = buildRcaPrompt({
        issueId: 1, title: 'Test', resource_id: 'r-1', severity: 'medium',
        regions: ['us-east-1'],
      });
      assert.ok(prompt.includes('CloudTrail') || prompt.includes('cloudtrail'));
      assert.ok(prompt.includes('指标') || prompt.includes('metric'));
      assert.ok(prompt.includes('日志') || prompt.includes('log'));
    });

    it('includes confidence scoring rubric', () => {
      const prompt = buildRcaPrompt({
        issueId: 1, title: 'Test', resource_id: 'r-1', severity: 'high',
        regions: [],
      });
      assert.ok(prompt.includes('置信度') || prompt.includes('confidence'));
    });

    it('includes fix risk level guidance', () => {
      const prompt = buildRcaPrompt({
        issueId: 1, title: 'Test', resource_id: 'r-1', severity: 'critical',
        regions: [],
      });
      assert.ok(prompt.includes('修复风险') || prompt.includes('risk'));
    });
  });

  describe('parseConfidence', () => {
    it('clamps to 0-1 range', () => {
      assert.equal(parseConfidence(1.5), 1.0);
      assert.equal(parseConfidence(-0.5), 0.0);
      assert.equal(parseConfidence(0.85), 0.85);
    });

    it('rounds to 2 decimal places', () => {
      assert.equal(parseConfidence(0.8567), 0.86);
    });

    it('defaults to 0.5 for NaN', () => {
      assert.equal(parseConfidence(NaN), 0.5);
      assert.equal(parseConfidence(undefined as any), 0.5);
    });
  });

  describe('createRcaResult', () => {
    it('creates a well-formed RcaResult', () => {
      const result = createRcaResult({
        issue_id: 42,
        root_cause: 'Memory leak in worker process caused OOM',
        confidence: 0.88,
        contributing_factors: ['High traffic spike', 'No memory limits set'],
        recommendations: ['Set memory limits', 'Add HPA'],
        fix_plan: { steps: ['kubectl set resources ...'] },
        fix_risk_level: 'medium',
        evidence: { metrics: { cpu: 95 }, logs: ['OOMKilled'] },
        model_id: 'claude-sonnet',
        duration_ms: 15000,
      });
      assert.equal(result.issue_id, 42);
      assert.equal(result.root_cause, 'Memory leak in worker process caused OOM');
      assert.equal(result.confidence, 0.88);
      assert.equal(result.fix_risk_level, 'medium');
      assert.equal(result.contributing_factors.length, 2);
      assert.equal(result.recommendations.length, 2);
      assert.ok(result.created_at);
    });

    it('clamps confidence to valid range', () => {
      const result = createRcaResult({
        issue_id: 1, root_cause: 'test', confidence: 1.5,
        contributing_factors: [], recommendations: [],
        fix_plan: {}, fix_risk_level: 'low', evidence: {},
        model_id: 'test', duration_ms: 100,
      });
      assert.equal(result.confidence, 1.0);
    });
  });
});
