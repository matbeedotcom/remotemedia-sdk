// Shared test helpers.
//
// Most specs need to: open the SPA with the test WS URL baked into the
// query string, grant mic permission, and click the start button. These
// helpers keep specs short.

import { expect, type Page } from '@playwright/test'
/* eslint-disable @typescript-eslint/no-explicit-any */

// Must match the default in playwright.config.ts (WS_PORT).
const WS_PORT = Number(process.env.WS_PORT ?? 18191)
export const WS_URL = `ws://127.0.0.1:${WS_PORT}/ws`

export async function openApp(page: Page) {
  await page.goto(`/?ws=${encodeURIComponent(WS_URL)}`)
  await expect(
    page.getByText('LFM2-Audio · WebRTC Observer'),
  ).toBeVisible()
}

export async function startMic(page: Page) {
  await page.getByRole('button', { name: /start mic/i }).click()
  // We always hit "connecting" at minimum; allow generous time for
  // ICE to settle on loopback (STUN → host candidate; usually fast).
  await expect(page.getByText(/^live$/)).toBeVisible({ timeout: 30_000 })
}

/// Read the Zustand store directly through the window bridge installed
/// by `installStoreBridge`. Returns the live state snapshot.
export async function readStore<T = unknown>(page: Page): Promise<T> {
  return (await page.evaluate(() =>
    // @ts-expect-error - window.__store is installed by the bridge.
    (window as Window & { __store: { getState: () => unknown } }).__store.getState(),
  )) as T
}

/// Install a `window.__store` reference to the Zustand store so tests
/// can observe state without scraping the DOM. Only affects test runs.
export async function installStoreBridge(page: Page) {
  await page.addInitScript(() => {
    // Expose the store as soon as main.tsx imports it. The store
    // module sets window.__store in its top-level side effect
    // (see src/store.ts — guarded behind a test hook).
    ;(window as unknown as { __TEST__: boolean }).__TEST__ = true
  })
}
