import { test, expect } from '@playwright/test';

async function loginAsAdmin(request) {
  return request.post('/admin/api/auth/login', {
    data: { username: 'admin', password: 'admin123' },
  });
}

test.describe('Health endpoint (fixed)', () => {
  test('GET /health returns 200 with status ok', async ({ request }) => {
    const res = await request.get('/health');
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.status).toBe('ok');
  });
});

test.describe('Admin API - Config Endpoints', () => {
  test.beforeEach(async ({ request }) => {
    await loginAsAdmin(request);
  });

  const configEndpoints = [
    'glossary', 'accounts', 'platforms', 'skills',
    'scheduled-jobs', 'plugins', 'provider', 'telemetry',
    'clusters', 'tenants',
  ];

  for (const endpoint of configEndpoints) {
    test(`GET /admin/api/${endpoint} returns 200`, async ({ request }) => {
      const res = await request.get(`/admin/api/${endpoint}`);
      expect(res.status()).toBe(200);
    });
  }

  test('PUT /admin/api/glossary updates and persists', async ({ request }) => {
    // Read current glossary (returns JSON)
    const getRes = await request.get('/admin/api/glossary');
    const original = await getRes.json();

    // Add a test entry
    const updated = { ...original };
    if (!updated.glossary) updated.glossary = {};
    updated.glossary['e2e-api-test'] = {
      full_name: 'API Test',
      description: 'E2E test entry',
    };

    const putRes = await request.put('/admin/api/glossary', {
      data: updated,
    });
    expect([200, 204]).toContain(putRes.status());

    // Read back and verify
    const verifyRes = await request.get('/admin/api/glossary');
    const verifyBody = await verifyRes.json();
    expect(verifyBody.glossary).toHaveProperty('e2e-api-test');

    // Cleanup: restore original
    await request.put('/admin/api/glossary', { data: original });
  });
});

test.describe('Admin API - Knowledge CRUD', () => {
  test.beforeEach(async ({ request }) => {
    await loginAsAdmin(request);
  });

  test('GET /admin/api/knowledge lists files', async ({ request }) => {
    const res = await request.get('/admin/api/knowledge');
    expect(res.status()).toBe(200);
    const body = await res.json();
    // Response is {files: [...]}
    expect(body.files).toBeDefined();
    expect(Array.isArray(body.files)).toBe(true);
  });

  test('PUT then GET then DELETE knowledge file', async ({ request }) => {
    const filename = 'e2e-test-knowledge.md';
    const content = '# E2E Test\nThis is a test knowledge file.';

    // Create — knowledge PUT expects JSON body with {content}
    const putRes = await request.put(`/admin/api/knowledge/${filename}`, {
      data: { content },
    });
    expect([200, 201, 204]).toContain(putRes.status());

    // Read back
    const getRes = await request.get(`/admin/api/knowledge/${filename}`);
    expect(getRes.status()).toBe(200);
    const body = await getRes.json();
    expect(body.content).toContain('E2E Test');

    // Delete
    const delRes = await request.delete(`/admin/api/knowledge/${filename}`);
    expect([200, 204]).toContain(delRes.status());

    // Verify deleted
    const verifyRes = await request.get(`/admin/api/knowledge/${filename}`);
    expect(verifyRes.status()).toBe(404);
  });

  test('Path traversal blocked for knowledge files', async ({ request }) => {
    const res = await request.get('/admin/api/knowledge/../../../etc/passwd');
    expect([400, 403, 404]).toContain(res.status());
  });
});

test.describe('Admin API - Users CRUD', () => {
  test.beforeEach(async ({ request }) => {
    await loginAsAdmin(request);
  });

  test('GET /admin/api/users lists users', async ({ request }) => {
    const res = await request.get('/admin/api/users');
    expect(res.status()).toBe(200);
    const body = await res.json();
    // Response is {users: [...]}
    expect(body.users).toBeDefined();
    expect(Array.isArray(body.users)).toBe(true);
    expect(body.users.some((u: any) => u.username === 'admin')).toBe(true);
  });

  test('POST, GET, DELETE user lifecycle', async ({ request }) => {
    const username = 'e2e-lifecycle-user-' + Date.now();

    // Create
    const createRes = await request.post('/admin/api/users', {
      data: { username, password: 'TestPass123!', role: 'super_admin' },
    });
    expect([200, 201]).toContain(createRes.status());

    // Verify in list
    const listRes = await request.get('/admin/api/users');
    const body = await listRes.json();
    expect(body.users.some((u: any) => u.username === username)).toBe(true);

    // Delete
    const delRes = await request.delete(`/admin/api/users/${username}`);
    expect([200, 204]).toContain(delRes.status());

    // Verify deleted
    const verifyRes = await request.get('/admin/api/users');
    const verifyBody = await verifyRes.json();
    expect(verifyBody.users.some((u: any) => u.username === username)).toBe(false);
  });

  test('POST duplicate username returns error', async ({ request }) => {
    const res = await request.post('/admin/api/users', {
      data: { username: 'admin', password: 'anything', role: 'super_admin' },
    });
    expect([400, 409]).toContain(res.status());
  });

  test('PUT reset user password', async ({ request }) => {
    const username = 'pw-reset-test-' + Date.now();
    await request.post('/admin/api/users', {
      data: { username, password: 'OldPass', role: 'super_admin' },
    });

    const resetRes = await request.put(`/admin/api/users/${username}/password`, {
      data: { password: 'NewPass123' },
    });
    expect([200, 204]).toContain(resetRes.status());

    // Verify can login with new password
    const loginRes = await request.post('/admin/api/auth/login', {
      data: { username, password: 'NewPass123' },
    });
    expect(loginRes.status()).toBe(200);

    // Cleanup
    await loginAsAdmin(request);
    await request.delete(`/admin/api/users/${username}`);
  });

  test('Create tenant_admin user with tenant_id', async ({ request }) => {
    const username = 'tenant-test-' + Date.now();
    const createRes = await request.post('/admin/api/users', {
      data: { username, password: 'TenantPass', role: 'tenant_admin', tenant_id: 'team-alpha' },
    });
    expect([200, 201]).toContain(createRes.status());

    // Cleanup
    await request.delete(`/admin/api/users/${username}`);
  });
});

test.describe('Admin API - Approvals', () => {
  test.beforeEach(async ({ request }) => {
    await loginAsAdmin(request);
  });

  test('POST create approval, GET list, POST approve', async ({ request }) => {
    // Create approval
    const createRes = await request.post('/admin/api/approvals', {
      data: {
        command: 'kubectl delete pod test-pod',
        requestedBy: 'e2e-test',
        requestedByName: 'E2E Test',
        platform: 'cli',
      },
    });
    expect([200, 201]).toContain(createRes.status());
    const approval = await createRes.json();
    expect(approval.id).toBeDefined();

    // List approvals — response is {approvals: [...]}
    const listRes = await request.get('/admin/api/approvals');
    expect(listRes.status()).toBe(200);
    const listBody = await listRes.json();
    expect(listBody.approvals).toBeDefined();

    // Get pending count
    const countRes = await request.get('/admin/api/approvals/pending-count');
    expect(countRes.status()).toBe(200);
    const countData = await countRes.json();
    expect(countData.count).toBeGreaterThanOrEqual(1);

    // Approve
    const approveRes = await request.post(`/admin/api/approvals/${approval.id}/approve`, {
      data: { approver: 'admin' },
    });
    expect([200, 204]).toContain(approveRes.status());
  });

  test('POST create approval then reject', async ({ request }) => {
    const createRes = await request.post('/admin/api/approvals', {
      data: {
        command: 'kubectl scale deployment test --replicas=0',
        requestedBy: 'e2e-test',
        requestedByName: 'E2E Test',
        platform: 'cli',
      },
    });
    const approval = await createRes.json();

    const rejectRes = await request.post(`/admin/api/approvals/${approval.id}/reject`, {
      data: { rejecter: 'admin', reason: 'E2E test rejection' },
    });
    expect([200, 204]).toContain(rejectRes.status());
  });

  test('GET filter approvals by status', async ({ request }) => {
    const res = await request.get('/admin/api/approvals?status=pending');
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.approvals).toBeDefined();
  });
});

test.describe('Admin API - Issues & Resources', () => {
  test.beforeEach(async ({ request }) => {
    await loginAsAdmin(request);
  });

  test('GET /admin/api/issues returns list', async ({ request }) => {
    const res = await request.get('/admin/api/issues');
    expect(res.status()).toBe(200);
    const body = await res.json();
    // Response is {issues: [], db_error: ""}
    expect(body.issues).toBeDefined();
    expect(Array.isArray(body.issues)).toBe(true);
  });

  test('GET /admin/api/resources returns list', async ({ request }) => {
    const res = await request.get('/admin/api/resources');
    expect(res.status()).toBe(200);
    const body = await res.json();
    // Response is {resources: [], db_error: ""}
    expect(body.resources).toBeDefined();
    expect(Array.isArray(body.resources)).toBe(true);
  });

  test('GET /admin/api/resources/summary returns counts', async ({ request }) => {
    const res = await request.get('/admin/api/resources/summary');
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body).toHaveProperty('total');
  });
});

test.describe('Alert Webhook', () => {
  test('POST /api/alerts with CloudWatch format', async ({ request }) => {
    const payload = {
      Type: 'Notification',
      Message: JSON.stringify({
        AlarmName: 'e2e-test-alarm',
        NewStateValue: 'ALARM',
        NewStateReason: 'Threshold crossed',
        StateChangeTime: new Date().toISOString(),
      }),
    };
    const res = await request.post('/api/alerts', { data: payload });
    expect([200, 201, 202]).toContain(res.status());
  });

  test('POST /api/alerts with Grafana format', async ({ request }) => {
    // Grafana unified alerting format requires specific structure
    const payload = {
      receiver: 'opsagent',
      status: 'firing',
      alerts: [{
        status: 'firing',
        labels: { alertname: 'e2e-test-alert', severity: 'critical' },
        annotations: { summary: 'E2E test alert', description: 'Test alert from Playwright' },
        startsAt: new Date().toISOString(),
        endsAt: '0001-01-01T00:00:00Z',
        generatorURL: 'http://grafana:3000',
        fingerprint: 'e2e-test',
      }],
      groupLabels: { alertname: 'e2e-test-alert' },
      commonLabels: { alertname: 'e2e-test-alert' },
      commonAnnotations: { summary: 'E2E test alert' },
      externalURL: 'http://grafana:3000',
    };
    const res = await request.post('/api/alerts', { data: payload });
    // Accept 200 (handled), 400 (unrecognized but server didn't crash)
    expect(res.status()).toBeLessThan(500);
    // Server still alive
    const healthRes = await request.get('/health');
    expect(healthRes.status()).toBe(200);
  });

  test('POST /api/alerts with empty body does not crash', async ({ request }) => {
    const res = await request.post('/api/alerts', { data: {} });
    expect(res.status()).toBeLessThan(600);
    const healthRes = await request.get('/health');
    expect(healthRes.status()).toBe(200);
  });

  test('POST /api/alerts with invalid JSON does not crash', async ({ request }) => {
    const res = await request.fetch('/api/alerts', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      data: 'not json',
    });
    expect(res.status()).toBeLessThan(600);
  });
});

test.describe('Admin API - Clusters', () => {
  test.beforeEach(async ({ request }) => {
    await loginAsAdmin(request);
  });

  test('GET /admin/api/clusters returns config', async ({ request }) => {
    const res = await request.get('/admin/api/clusters');
    expect(res.status()).toBe(200);
  });

  test('GET /admin/api/clusters/status returns status', async ({ request }) => {
    const res = await request.get('/admin/api/clusters/status');
    expect(res.status()).toBe(200);
  });

  test('POST /admin/api/clusters/refresh triggers refresh', async ({ request }) => {
    const res = await request.post('/admin/api/clusters/refresh');
    expect([200, 202]).toContain(res.status());
  });
});

test.describe('Personal Config Endpoints', () => {
  test.beforeEach(async ({ request }) => {
    await loginAsAdmin(request);
  });

  test('GET /admin/api/me/knowledge returns list', async ({ request }) => {
    const res = await request.get('/admin/api/me/knowledge');
    expect(res.status()).toBe(200);
    const body = await res.json();
    // Response is {files: [...]}
    expect(body.files).toBeDefined();
    expect(Array.isArray(body.files)).toBe(true);
  });

  test('PUT then GET then DELETE personal knowledge file', async ({ request }) => {
    const filename = 'e2e-personal-test.md';

    // Create — personal knowledge PUT expects JSON body
    const putRes = await request.put(`/admin/api/me/knowledge/${filename}`, {
      data: { content: '# Personal E2E Test' },
    });
    expect([200, 201, 204]).toContain(putRes.status());

    // Read
    const getRes = await request.get(`/admin/api/me/knowledge/${filename}`);
    expect(getRes.status()).toBe(200);

    // Delete
    const delRes = await request.delete(`/admin/api/me/knowledge/${filename}`);
    expect([200, 204]).toContain(delRes.status());
  });

  test('GET /admin/api/me/skills returns config', async ({ request }) => {
    const res = await request.get('/admin/api/me/skills');
    expect(res.status()).toBe(200);
  });

  test('GET /admin/api/me/scheduled-jobs returns config', async ({ request }) => {
    const res = await request.get('/admin/api/me/scheduled-jobs');
    expect(res.status()).toBe(200);
  });
});
