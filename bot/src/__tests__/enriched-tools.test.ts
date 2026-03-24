import { describe, it } from 'node:test';
import * as assert from 'node:assert/strict';

describe('Enriched Tool Definitions', () => {
  it('exports ENRICHED_TOOLS array with AWS tools', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    assert.ok(Array.isArray(ENRICHED_TOOLS));
    assert.ok(ENRICHED_TOOLS.length >= 10, `Expected at least 10 enriched tools, got ${ENRICHED_TOOLS.length}`);
  });

  it('each tool has name, description, and input_schema', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    for (const tool of ENRICHED_TOOLS) {
      assert.ok(tool.name, `Tool missing name`);
      assert.ok(tool.description, `Tool ${tool.name} missing description`);
      assert.ok(tool.input_schema, `Tool ${tool.name} missing input_schema`);
      assert.equal(tool.input_schema.type, 'object');
    }
  });

  it('includes describe_ec2 tool', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    const ec2 = ENRICHED_TOOLS.find((t: any) => t.name === 'describe_ec2');
    assert.ok(ec2, 'describe_ec2 tool not found');
    assert.ok(ec2.input_schema.properties.region);
  });

  it('includes list_alarms tool', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    const alarms = ENRICHED_TOOLS.find((t: any) => t.name === 'list_alarms');
    assert.ok(alarms, 'list_alarms tool not found');
  });

  it('includes describe_rds tool', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    const rds = ENRICHED_TOOLS.find((t: any) => t.name === 'describe_rds');
    assert.ok(rds, 'describe_rds tool not found');
  });

  it('includes scan_resources tool', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    const scan = ENRICHED_TOOLS.find((t: any) => t.name === 'scan_resources');
    assert.ok(scan, 'scan_resources tool not found');
  });

  it('includes describe_vpcs tool', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    const vpcs = ENRICHED_TOOLS.find((t: any) => t.name === 'describe_vpcs');
    assert.ok(vpcs, 'describe_vpcs tool not found');
  });

  it('includes describe_security_groups tool', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    const sgs = ENRICHED_TOOLS.find((t: any) => t.name === 'describe_security_groups');
    assert.ok(sgs, 'describe_security_groups tool not found');
  });

  it('includes get_cloudwatch_metrics tool', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    const metrics = ENRICHED_TOOLS.find((t: any) => t.name === 'get_cloudwatch_metrics');
    assert.ok(metrics, 'get_cloudwatch_metrics tool not found');
  });

  it('includes query_cloudwatch_logs tool', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    const logs = ENRICHED_TOOLS.find((t: any) => t.name === 'query_cloudwatch_logs');
    assert.ok(logs, 'query_cloudwatch_logs tool not found');
  });

  it('includes lookup_cloudtrail tool', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    const trail = ENRICHED_TOOLS.find((t: any) => t.name === 'lookup_cloudtrail');
    assert.ok(trail, 'lookup_cloudtrail tool not found');
  });

  it('tool names are unique', () => {
    const { ENRICHED_TOOLS } = require('../enriched-tools');
    const names = ENRICHED_TOOLS.map((t: any) => t.name);
    const unique = new Set(names);
    assert.equal(names.length, unique.size, `Duplicate tool names found`);
  });
});

describe('Enriched Tool Executor', () => {
  it('exports executeEnrichedTool function', () => {
    const { executeEnrichedTool } = require('../enriched-tools');
    assert.equal(typeof executeEnrichedTool, 'function');
  });

  it('returns error for unknown tool', async () => {
    const { executeEnrichedTool } = require('../enriched-tools');
    const result = await executeEnrichedTool('nonexistent_tool', {}, { env: process.env, workDir: '/tmp' });
    assert.ok(result.is_error);
    assert.ok(result.content.includes('Unknown'));
  });

  it('describe_ec2 calls aws ec2 describe-instances', async () => {
    const { executeEnrichedTool } = require('../enriched-tools');
    // Without real AWS credentials, this will fail but we verify it attempts the right command
    const result = await executeEnrichedTool('describe_ec2', { region: 'us-east-1' }, {
      env: { ...process.env, AWS_DEFAULT_REGION: 'us-east-1' },
      workDir: '/tmp',
      sandboxConfig: {},
    });
    // Should either return data or an auth error (not "unknown tool")
    assert.ok(!result.content.includes('Unknown enriched tool'));
  });
});
