/**
 * OpsAgent K8s Query E2E Tests
 *
 * Tests various Kubernetes questions through the Chat API (streaming).
 * Validates that Claude correctly invokes kubectl and returns structured results.
 *
 * Prerequisites: Bedrock/Claude credentials configured, EKS clusters accessible.
 */
import { test, expect } from '@playwright/test';

const CHAT_TIMEOUT = 120_000; // 2 minutes per query

/** Login and return cookies for subsequent requests */
async function login(request) {
  await request.post('/admin/api/auth/login', {
    data: { username: 'admin', password: 'admin123' },
  });
}

/**
 * Send a chat message via streaming API and collect the full response.
 * Returns { text, events, durationMs }.
 */
async function chatQuery(request, message: string, tenantId?: string): Promise<{
  text: string;
  events: Array<{ type: string; content: string }>;
  durationMs: number;
  hasToolUse: boolean;
  hasError: boolean;
}> {
  const body: any = { message };
  if (tenantId) body.tenantId = tenantId;

  const res = await request.post('/admin/api/chat/stream', {
    data: body,
    timeout: CHAT_TIMEOUT,
  });

  const rawText = await res.text();
  const events: Array<{ type: string; content: string }> = [];
  let fullText = '';
  let durationMs = 0;
  let hasToolUse = false;
  let hasError = false;

  // Parse SSE events
  for (const block of rawText.split('\n\n')) {
    const dataLine = block.split('\n').find(l => l.startsWith('data:'));
    if (!dataLine) continue;
    try {
      const data = JSON.parse(dataLine.substring(5));
      events.push({ type: data.type, content: data.content || '' });
      if (data.type === 'text') fullText += data.content;
      if (data.type === 'tool_use') hasToolUse = true;
      if (data.type === 'error') hasError = true;
      if (data.type === 'done') durationMs = data.durationMs || 0;
    } catch { /* skip malformed lines */ }
  }

  return { text: fullText, events, durationMs, hasToolUse, hasError };
}

// ── Test Setup ──────────────────────────────────────────────────
test.beforeEach(async ({ request }) => {
  await login(request);
});

// Extend timeout for all tests in this file (K8s queries take time)
test.describe.configure({ timeout: CHAT_TIMEOUT });

// ================================================================
// 1. Basic Cluster Queries
// ================================================================
test.describe('Basic Cluster Queries', () => {

  test('查看所有集群的 namespace', async ({ request }) => {
    const result = await chatQuery(request, '帮我看一下所有集群有哪些 namespace');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true); // Should use kubectl
    expect(result.text).toContain('namespace');
    // Should mention at least one known cluster
    expect(
      result.text.includes('ecommerce-prod') ||
      result.text.includes('legacy-migration') ||
      result.text.includes('data-platform')
    ).toBe(true);
  });

  test('查看所有集群节点状态', async ({ request }) => {
    const result = await chatQuery(request, '各个集群的节点状态怎么样？');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    // Should contain node info (Ready/NotReady or node names)
    expect(
      result.text.includes('Ready') ||
      result.text.includes('节点') ||
      result.text.includes('node')
    ).toBe(true);
  });

  test('查看所有集群的 pod 数量', async ({ request }) => {
    const result = await chatQuery(request, '帮我统计每个集群有多少个 pod');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    // Should return numbers
    expect(result.text).toMatch(/\d+/);
  });
});

// ================================================================
// 2. Specific Cluster Queries
// ================================================================
test.describe('Specific Cluster Queries', () => {

  test('查看 ecommerce-prod 集群的 pods', async ({ request }) => {
    const result = await chatQuery(request, '看一下 ecommerce-prod 集群所有 pod 的状态');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    expect(result.text).toContain('ecommerce');
    // Should show pod status
    expect(
      result.text.includes('Running') ||
      result.text.includes('pod') ||
      result.text.includes('Pod')
    ).toBe(true);
  });

  test('查看 legacy-migration 的 core-banking 服务', async ({ request }) => {
    const result = await chatQuery(request, 'legacy-migration 集群 core-banking namespace 里有哪些服务?');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    expect(
      result.text.includes('core-banking') ||
      result.text.includes('legacy')
    ).toBe(true);
  });
});

// ================================================================
// 3. Troubleshooting Queries
// ================================================================
test.describe('Troubleshooting Queries', () => {

  test('检查是否有异常 pod', async ({ request }) => {
    const result = await chatQuery(request, '有没有哪些 pod 不是 Running 状态的？');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    // Should give a clear answer about pod status
    expect(result.text.length).toBeGreaterThan(20);
  });

  test('检查 pod 重启次数', async ({ request }) => {
    const result = await chatQuery(request, '帮我看一下哪些 pod 重启次数比较多');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    expect(
      result.text.includes('重启') ||
      result.text.includes('restart') ||
      result.text.includes('RESTARTS')
    ).toBe(true);
  });

  test('检查 deployment 的 replicas', async ({ request }) => {
    const result = await chatQuery(request, '所有集群的 deployment，有没有 replicas 不满的？');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    expect(result.text.length).toBeGreaterThan(20);
  });
});

// ================================================================
// 4. Resource Queries
// ================================================================
test.describe('Resource Queries', () => {

  test('查看 service 和 ingress', async ({ request }) => {
    const result = await chatQuery(request, '帮我看看 ecommerce-prod 集群有哪些 service 和 ingress');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    expect(
      result.text.includes('Service') ||
      result.text.includes('service') ||
      result.text.includes('ClusterIP') ||
      result.text.includes('LoadBalancer')
    ).toBe(true);
  });

  test('查看 configmap 和 secret 数量', async ({ request }) => {
    const result = await chatQuery(request, 'ecommerce-prod 每个 namespace 有多少 configmap 和 secret');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    expect(result.text).toMatch(/\d+/);
  });

  test('查看 PVC 使用情况', async ({ request }) => {
    const result = await chatQuery(request, '所有集群的 PVC 状态和容量');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    expect(result.text.length).toBeGreaterThan(20);
  });
});

// ================================================================
// 5. Company Terminology Queries (Glossary Integration)
// ================================================================
test.describe('Company Terminology Queries', () => {

  test('用公司术语查询: ics 系统状态', async ({ request }) => {
    const result = await chatQuery(request, 'ics 系统现在正常吗？');
    expect(result.hasError).toBe(false);
    // Should resolve "ics" -> Inventory Control System and query ecommerce-prod
    expect(
      result.text.includes('Inventory') ||
      result.text.includes('ecommerce') ||
      result.text.includes('shop-backend') ||
      result.text.includes('ICS') ||
      result.text.includes('ics')
    ).toBe(true);
  });

  test('用公司术语查询: cbs 迁移进度', async ({ request }) => {
    const result = await chatQuery(request, 'cbs 的 pod 状态怎么样？');
    expect(result.hasError).toBe(false);
    // Should resolve "cbs" -> Core Banking System, query legacy-migration cluster
    expect(
      result.text.includes('core-banking') ||
      result.text.includes('legacy') ||
      result.text.includes('CBS') ||
      result.text.includes('banking')
    ).toBe(true);
  });

  test('用公司术语查询: datalake 平台', async ({ request }) => {
    const result = await chatQuery(request, 'datalake 集群的健康状态');
    expect(result.hasError).toBe(false);
    // Should resolve "datalake" -> Data Lake Platform, query data-platform cluster
    expect(
      result.text.includes('data-platform') ||
      result.text.includes('Data Lake') ||
      result.text.includes('datalake') ||
      result.text.includes('data platform')
    ).toBe(true);
  });
});

// ================================================================
// 6. Cross-Cluster Comparison
// ================================================================
test.describe('Cross-Cluster Comparison', () => {

  test('比较所有集群的资源使用', async ({ request }) => {
    const result = await chatQuery(request, '帮我对比一下各集群的 pod 数量和节点数');
    expect(result.hasError).toBe(false);
    expect(result.hasToolUse).toBe(true);
    // Should have a comparison table or multiple cluster mentions
    expect(result.text.length).toBeGreaterThan(50);
    // Should mention the "集群" column as required by CLAUDE.md
    expect(
      result.text.includes('集群') ||
      result.text.includes('Cluster') ||
      result.text.includes('ecommerce') ||
      result.text.includes('legacy')
    ).toBe(true);
  });
});

// ================================================================
// 7. Edge Cases
// ================================================================
test.describe('Edge Cases', () => {

  test('查询不存在的集群', async ({ request }) => {
    const result = await chatQuery(request, '帮我查一下 nonexistent-cluster 的 pod 状态');
    expect(result.hasError).toBe(false);
    // Should handle gracefully - either say cluster not found or suggest available clusters
    expect(result.text.length).toBeGreaterThan(10);
  });

  test('查询不存在的 namespace', async ({ request }) => {
    const result = await chatQuery(request, 'ecommerce-prod 集群里 nonexistent-ns namespace 有什么');
    expect(result.hasError).toBe(false);
    expect(result.text.length).toBeGreaterThan(10);
  });

  test('模糊查询: 只说看看集群', async ({ request }) => {
    const result = await chatQuery(request, '看看集群');
    expect(result.hasError).toBe(false);
    // Should query all clusters as per CLAUDE.md instruction
    expect(result.hasToolUse).toBe(true);
    expect(result.text.length).toBeGreaterThan(20);
  });
});
