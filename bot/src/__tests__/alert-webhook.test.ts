import { describe, it } from 'node:test';
import * as assert from 'node:assert/strict';
import {
  parseCloudWatchAlert, parseDatadogAlert, parseGenericAlert,
  normalizeAlert, AlertPayload,
} from '../alert-webhook';

describe('Alert Webhook', () => {
  describe('parseCloudWatchAlert', () => {
    it('parses SNS-wrapped CloudWatch alarm', () => {
      const payload = {
        Type: 'Notification',
        Message: JSON.stringify({
          AlarmName: 'HighCPU-i-abc123',
          NewStateValue: 'ALARM',
          NewStateReason: 'Threshold crossed: CPU > 90%',
          Trigger: {
            MetricName: 'CPUUtilization',
            Namespace: 'AWS/EC2',
            Dimensions: [{ name: 'InstanceId', value: 'i-abc123' }],
          },
        }),
      };
      const alert = parseCloudWatchAlert(payload);
      assert.ok(alert);
      assert.equal(alert!.source, 'cloudwatch');
      assert.equal(alert!.title, 'HighCPU-i-abc123');
      assert.equal(alert!.severity, 'high');
      assert.equal(alert!.resource_id, 'i-abc123');
    });

    it('returns null for non-alarm payload', () => {
      const alert = parseCloudWatchAlert({ Type: 'SubscriptionConfirmation' });
      assert.equal(alert, null);
    });
  });

  describe('parseDatadogAlert', () => {
    it('parses Datadog webhook payload', () => {
      const payload = {
        title: 'CPU above 90% on i-abc123',
        alert_type: 'error',
        priority: 'P1',
        tags: 'instance:i-abc123,env:prod',
        body: 'CPU utilization exceeded threshold',
      };
      const alert = parseDatadogAlert(payload);
      assert.ok(alert);
      assert.equal(alert!.source, 'datadog');
      assert.ok(alert!.title.includes('CPU'));
      assert.equal(alert!.severity, 'critical'); // P1 = critical
    });

    it('maps Datadog priority to severity', () => {
      const p1 = parseDatadogAlert({ title: 't', alert_type: 'error', priority: 'P1' });
      const p2 = parseDatadogAlert({ title: 't', alert_type: 'warning', priority: 'P2' });
      const p3 = parseDatadogAlert({ title: 't', alert_type: 'info', priority: 'P3' });
      assert.equal(p1!.severity, 'critical');
      assert.equal(p2!.severity, 'high');
      assert.equal(p3!.severity, 'medium');
    });
  });

  describe('parseGenericAlert', () => {
    it('parses a generic webhook with title and severity', () => {
      const alert = parseGenericAlert({
        title: 'Disk full on db-primary',
        severity: 'critical',
        resource_id: 'db-primary',
        description: 'Disk usage at 98%',
      });
      assert.ok(alert);
      assert.equal(alert!.source, 'webhook');
      assert.equal(alert!.severity, 'critical');
    });

    it('returns null if title is missing', () => {
      const alert = parseGenericAlert({ severity: 'high' });
      assert.equal(alert, null);
    });
  });

  describe('normalizeAlert', () => {
    it('auto-detects CloudWatch format', () => {
      const alert = normalizeAlert({ Type: 'Notification', Message: JSON.stringify({ AlarmName: 'test', NewStateValue: 'ALARM', Trigger: {} }) });
      assert.ok(alert);
      assert.equal(alert!.source, 'cloudwatch');
    });

    it('auto-detects Datadog format', () => {
      const alert = normalizeAlert({ title: 'test', alert_type: 'error' });
      assert.ok(alert);
      assert.equal(alert!.source, 'datadog');
    });

    it('falls back to generic format', () => {
      const alert = normalizeAlert({ title: 'custom alert', severity: 'medium' });
      assert.ok(alert);
      assert.equal(alert!.source, 'webhook');
    });

    it('returns null for empty payload', () => {
      assert.equal(normalizeAlert({}), null);
      assert.equal(normalizeAlert(null as any), null);
    });
  });
});
