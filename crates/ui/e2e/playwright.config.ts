import { defineConfig, devices } from '@playwright/test';
import path from 'path';

const UI_PORT = process.env.UI_PORT || '3001';
const WS_PORT = process.env.WS_PORT || '18091';
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
    // Make WS signaling URL available to tests
    extraHTTPHeaders: {},
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],

  // Start the CLI with WebRTC transport + WS signaling + UI.
  // This enables both the UI tests and WebRTC signaling tests.
  webServer: {
    command: `cargo run --features ui,webrtc -- serve ${MANIFEST} --transport webrtc --port 18080 --ws-port ${WS_PORT} --ui --ui-port ${UI_PORT}`,
    cwd: CLI_DIR,
    url: `http://127.0.0.1:${UI_PORT}/api/status`,
    reuseExistingServer: !process.env.CI,
    timeout: 180000, // 3 min for cargo build + start
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
