import { defineConfig, devices } from '@playwright/test';
import path from 'path';

const UI_PORT = process.env.UI_PORT || '3001';
const CLI_DIR = path.resolve(__dirname, '../../../examples/cli/remotemedia-cli');
const MANIFEST = path.resolve(__dirname, 'fixtures/passthrough.json');

export default defineConfig({
  testDir: './tests',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: 'html',
  timeout: 60000,

  use: {
    baseURL: `http://127.0.0.1:${UI_PORT}`,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  // Start the UI server before tests.
  // Builds and runs the CLI with the --ui flag pointing at a passthrough pipeline.
  webServer: {
    command: `cargo run --features ui -- serve ${MANIFEST} --ui --ui-port ${UI_PORT} --port 18080`,
    cwd: CLI_DIR,
    url: `http://127.0.0.1:${UI_PORT}/api/status`,
    reuseExistingServer: !process.env.CI,
    timeout: 180000, // 3 min for cargo build + start
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
