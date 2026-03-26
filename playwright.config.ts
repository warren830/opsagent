import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  timeout: 60_000,
  retries: 0,
  use: {
    baseURL: 'http://localhost:3978',
    screenshot: 'on',
    viewport: { width: 1280, height: 800 },
  },
  outputDir: './test-results',
  webServer: {
    command: 'node bot/dist/index.js',
    port: 3978,
    reuseExistingServer: !process.env.CI,
    env: {
      MICROSOFT_APP_ID: '',
      MICROSOFT_APP_PASSWORD: '',
      PORT: '3978',
      WORK_DIR: process.cwd(),
    },
  },
});
