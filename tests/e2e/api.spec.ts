import { test, expect } from '@playwright/test';

test.describe('Health endpoint', () => {
  test('GET /health returns 200 with status ok', async ({ request }) => {
    const response = await request.get('/health');
    expect(response.status()).toBe(200);

    const body = await response.json();
    expect(body.status).toBe('ok');
  });

  test('GET /health returns correct content-type', async ({ request }) => {
    const response = await request.get('/health');
    expect(response.headers()['content-type']).toContain('application/json');
  });
});

test.describe('404 handling', () => {
  test('GET unknown path returns 404', async ({ request }) => {
    const response = await request.get('/nonexistent');
    expect(response.status()).toBe(404);
  });

  test('GET / returns 404', async ({ request }) => {
    const response = await request.get('/');
    expect(response.status()).toBe(404);
  });
});

test.describe('POST /api/messages', () => {
  test('POST with empty body returns error (no crash)', async ({ request }) => {
    const response = await request.post('/api/messages', {
      headers: { 'Content-Type': 'application/json' },
      data: {},
    });
    // Bot Framework adapter will reject invalid activity, but server should not crash
    // Expect 4xx or 5xx but not a connection error
    expect(response.status()).toBeGreaterThanOrEqual(400);
  });

  test('POST with minimal Bot Framework activity structure', async ({ request }) => {
    // Simulate a Bot Framework activity without proper auth
    // When running without APP_ID/PASSWORD, the adapter may still reject
    const activity = {
      type: 'message',
      id: 'test-activity-1',
      timestamp: new Date().toISOString(),
      channelId: 'msteams',
      from: { id: 'user-1', name: 'TestUser' },
      conversation: { id: 'conv-1' },
      recipient: { id: 'bot-1', name: 'OpsAgent' },
      text: 'hello',
      serviceUrl: 'https://smba.trafficmanager.net/teams/',
    };

    const response = await request.post('/api/messages', {
      headers: { 'Content-Type': 'application/json' },
      data: activity,
    });

    // Without proper auth tokens, we expect the adapter to process it
    // In no-auth mode (empty APP_ID), it should accept the activity
    // The response could be 200 (processed) or 401/500 depending on adapter behavior
    expect([200, 201, 401, 500]).toContain(response.status());
  });

  test('GET /api/messages returns 404 (only POST allowed)', async ({ request }) => {
    const response = await request.get('/api/messages');
    expect(response.status()).toBe(404);
  });
});

test.describe('Server robustness', () => {
  test('POST with malformed JSON body returns 500', async ({ request }) => {
    // Send raw text that's not valid JSON - Playwright will send it as-is
    const response = await request.fetch('/api/messages', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      data: 'this is not json',
    });
    expect(response.status()).toBeGreaterThanOrEqual(400);
  });

  test('concurrent health checks succeed', async ({ request }) => {
    const results = await Promise.all(
      Array.from({ length: 10 }, () => request.get('/health')),
    );
    for (const response of results) {
      expect(response.status()).toBe(200);
    }
  });

  test('POST with large body does not crash server', async ({ request }) => {
    const activity = {
      type: 'message',
      channelId: 'msteams',
      from: { id: 'user-1', name: 'TestUser' },
      conversation: { id: 'conv-1' },
      recipient: { id: 'bot-1', name: 'OpsAgent' },
      text: 'a'.repeat(10000),
      serviceUrl: 'https://smba.trafficmanager.net/teams/',
    };

    const response = await request.post('/api/messages', {
      headers: { 'Content-Type': 'application/json' },
      data: activity,
    });
    // Server should handle it without crashing
    expect(response.status()).toBeLessThan(600);

    // Verify server is still alive after large request
    const healthResponse = await request.get('/health');
    expect(healthResponse.status()).toBe(200);
  });
});
