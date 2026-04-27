import { defineConfig, devices } from '@playwright/test'
import path from 'path'
import { fileURLToPath } from 'url'

const HERE = path.dirname(fileURLToPath(import.meta.url))
// Use dedicated ports so `npm test` doesn't collide with a
// `npm run dev` / example server the developer has running.
const UI_PORT = Number(process.env.UI_PORT ?? 5273)
const WS_PORT = Number(process.env.WS_PORT ?? 18191)
const REPO_ROOT = path.resolve(HERE, '../../..')

export default defineConfig({
  testDir: './e2e',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: 'list',
  timeout: 60_000,
  expect: { timeout: 15_000 },

  use: {
    baseURL: `http://127.0.0.1:${UI_PORT}`,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
  },

  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        launchOptions: {
          args: [
            // Fake mic: Chromium generates a 440 Hz sine wave — enough
            // to trip VAD in the test pipeline so control-bus
            // subscriptions have traffic to verify.
            '--use-fake-device-for-media-stream',
            '--use-fake-ui-for-media-stream',
            // Allow WebRTC ICE on loopback (required for localhost).
            '--allow-loopback-in-peer-connection',
          ],
        },
        // Grant mic permission up-front so tests don't hang on a prompt.
        contextOptions: { permissions: ['microphone'] },
      },
    },
  ],

  // Two webServers: (1) the pure-Rust WebRTC signaling server with a
  // VAD-only pipeline, (2) the Vite dev server hosting the SPA. The
  // SPA reads the WS URL from the `?ws=` query param (wired in the
  // Zustand store), so each test opens the app with that override.
  webServer: [
    {
      // tokio-tungstenite responds to a plain GET (no Upgrade) with
      // an HTTP 4xx — Playwright treats any HTTP response as "up",
      // which is what we need for a pure-WS server.
      command: `cargo run --example webrtc_test_server -p remotemedia-webrtc --features ws-signaling -- --port ${WS_PORT}`,
      cwd: REPO_ROOT,
      // TCP port probe — a pure-WS server rejects Playwright's plain
      // HTTP readiness GET ("No Connection: upgrade header"), so we
      // can't use `url` here. `port` just checks that something is
      // listening, which is what we need.
      port: WS_PORT,
      reuseExistingServer: !process.env.CI,
      timeout: 300_000,
      stdout: 'pipe',
      stderr: 'pipe',
    },
    {
      // Bind to 127.0.0.1 explicitly so Playwright's IPv4 baseURL
      // hits the same socket Vite is listening on (default is
      // localhost which resolves to ::1 on macOS).
      command: `npm run dev -- --host 127.0.0.1 --port ${UI_PORT} --strictPort`,
      cwd: HERE,
      port: UI_PORT,
      reuseExistingServer: !process.env.CI,
      timeout: 120_000,
      stdout: 'pipe',
      stderr: 'pipe',
    },
  ],
})
