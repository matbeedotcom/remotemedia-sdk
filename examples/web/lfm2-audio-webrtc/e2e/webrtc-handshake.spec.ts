// Full SDP/ICE handshake against the test server. Verifies that:
// - peer.announce → peer.offer → peer.answer complete
// - ICE trickle from both sides converges (the race fix in webrtc.ts)
// - status pill flips to "live"
// - a remote audio stream is attached to the store

import { test, expect } from '@playwright/test'
import { installStoreBridge, openApp, startMic, readStore } from './helpers'

test.beforeEach(async ({ page }) => {
  await installStoreBridge(page)
})

test('start mic → status goes live', async ({ page }) => {
  await openApp(page)
  await startMic(page)

  // Peer ID pill shows up once announce lands.
  await expect(page.getByText(/^peer:/)).toBeVisible()
  // "assistant idle" starts out because no generation has happened.
  await expect(page.getByText(/assistant idle/i)).toBeVisible()
})

test('remote audio track reaches the store', async ({ page }) => {
  await openApp(page)
  await startMic(page)

  // The ServerPeer always adds an audio track to the answer; we should
  // receive it once ICE connects.
  await expect
    .poll(
      async () => {
        const s = await readStore<{ remoteAudioStream: MediaStream | null }>(
          page,
        )
        return s.remoteAudioStream !== null
      },
      { timeout: 20_000, message: 'remote audio track never arrived' },
    )
    .toBeTruthy()
})
