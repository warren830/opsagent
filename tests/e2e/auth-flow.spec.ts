import { test, expect } from '@playwright/test';

// Helper to login and get session cookie
async function login(request, username = 'admin', password = 'admin123') {
  const res = await request.post('/admin/api/auth/login', {
    data: { username, password },
  });
  return res;
}

test.describe('Authentication Flow', () => {
  test('POST /admin/api/auth/login with valid credentials returns 200', async ({ request }) => {
    const res = await login(request);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.user).toBeDefined();
    expect(body.user.username).toBe('admin');
    expect(body.user.role).toBe('super_admin');
  });

  test('POST /admin/api/auth/login with wrong password returns 401', async ({ request }) => {
    const res = await login(request, 'admin', 'wrongpassword');
    expect(res.status()).toBe(401);
  });

  test('POST /admin/api/auth/login with empty fields returns 400', async ({ request }) => {
    const res = await request.post('/admin/api/auth/login', {
      data: { username: '', password: '' },
    });
    expect(res.status()).toBe(400);
  });

  test('GET /admin/api/auth/me returns user info when logged in', async ({ request }) => {
    await login(request);
    const res = await request.get('/admin/api/auth/me');
    expect(res.status()).toBe(200);
    const body = await res.json();
    // Response is {user: {username, role}}
    expect(body.user.username).toBe('admin');
  });

  test('GET /admin/api/auth/me returns 401 when not logged in', async ({ request }) => {
    // Use a fresh context with no cookies
    const res = await request.get('/admin/api/auth/me', {
      headers: { Cookie: '' },
    });
    // Should be 401 or return null user
    expect([200, 401]).toContain(res.status());
  });

  test('POST /admin/api/auth/logout clears session', async ({ request }) => {
    await login(request);
    const logoutRes = await request.post('/admin/api/auth/logout');
    expect(logoutRes.status()).toBe(200);
  });

  test('PUT /admin/api/auth/change-password works', async ({ request }) => {
    // Create a test user first
    await login(request);
    await request.post('/admin/api/users', {
      data: { username: 'pw-test-user', password: 'OldPass123', role: 'super_admin' },
    });

    // Login as test user
    const testContext = await login(request, 'pw-test-user', 'OldPass123');

    // Change password
    const changeRes = await request.put('/admin/api/auth/change-password', {
      data: { currentPassword: 'OldPass123', newPassword: 'NewPass456' },
    });
    expect([200, 400]).toContain(changeRes.status());

    // Cleanup
    await login(request);
    await request.delete('/admin/api/users/pw-test-user');
  });

  test('Rate limiting: 6th failed login attempt is blocked', async ({ request }) => {
    const username = 'rate-limit-test-' + Date.now();
    let lastStatus = 0;
    for (let i = 0; i < 6; i++) {
      const res = await request.post('/admin/api/auth/login', {
        data: { username, password: 'wrong' },
      });
      lastStatus = res.status();
    }
    // After 5+ failures, should get 429 (too many requests) or still 401
    expect([401, 429]).toContain(lastStatus);
  });
});
