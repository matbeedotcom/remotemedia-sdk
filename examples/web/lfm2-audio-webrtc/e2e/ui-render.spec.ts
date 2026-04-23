// Smoke test — page loads, core panels render, status starts 'idle'.
// Doesn't touch the mic or WebRTC. Fastest test in the suite; run
// this first to catch bundling / template breakage early.

import { test, expect } from '@playwright/test'
import { installStoreBridge, openApp } from './helpers'

test.beforeEach(async ({ page }) => {
  await installStoreBridge(page)
})

test('renders header, knowledge pane, transcript panes', async ({ page }) => {
  await openApp(page)

  await expect(
    page.getByText('LFM2-Audio · WebRTC Observer'),
  ).toBeVisible()
  await expect(
    page.getByRole('button', { name: /start mic/i }),
  ).toBeEnabled()
  await expect(
    page.getByText(/knowledge injection/i),
  ).toBeVisible()
  await expect(
    page.getByText(/current turn/i),
  ).toBeVisible()
  await expect(
    page.getByText(/history \(0\)/i),
  ).toBeVisible()
})

test('inject button is disabled while disconnected', async ({ page }) => {
  await openApp(page)
  const textarea = page
    .getByPlaceholder(/the user's name is mathieu/i)
    .first()
  await textarea.fill('test knowledge')
  await expect(page.getByRole('button', { name: /^inject$/i })).toBeDisabled()
})
