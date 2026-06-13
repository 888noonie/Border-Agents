/// <reference types="node" />

import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright config for Border Agents.
 * Designed for agent-driven development + E2E of the web layer.
 *
 * Usage (agents / humans):
 *   npx playwright test                 # runs against dev server (auto-starts if needed)
 *   npx playwright test --ui            # interactive UI mode (great for exploration)
 *   npx playwright codegen http://127.0.0.1:1420
 *   npx playwright show-trace test-results/...
 *
 * The webServer block auto-launches `npm run dev` for reliable agent runs.
 * Tests target the Vite-served React UI (the surface that becomes the Tauri webview).
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: [
    ['list'],
    ['html', { outputFolder: 'playwright-report', open: 'never' }],
  ],

  use: {
    baseURL: 'http://127.0.0.1:1420',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    // Helpful for slow Linux/CI boxes and Tauri-like webviews
    actionTimeout: 10_000,
    navigationTimeout: 15_000,
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    // Future expansion:
    // { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    // { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],

  // Auto-starts the Vite dev server for `npx playwright test`.
  // Agents love this: no need to remember to run `npm run dev` first.
  webServer: {
    command: 'npm run dev',
    url: 'http://127.0.0.1:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 120 * 1000,
    stdout: 'ignore',
    stderr: 'pipe',
  },
});
