// Exercises the `control.*` JSON-RPC surface:
//   - pre-announce call is rejected
//   - control.publish round-trips through the SPA's inject path
//   - control.subscribe forwards vad.out events into the store

import { test, expect } from '@playwright/test'

// Must match the default in playwright.config.ts (WS_PORT).
const WS_PORT = Number(process.env.WS_PORT ?? 18191)
const WS_URL = `ws://127.0.0.1:${WS_PORT}/ws`

test('control.* requires peer.announce first', async ({ page }) => {
  await page.goto('about:blank')
  const err = await page.evaluate(
    async ({ url }) => {
      const ws = new WebSocket(url)
      await new Promise((res) => (ws.onopen = res))
      return await new Promise<string | null>((resolve) => {
        const id = 7
        ws.onmessage = (ev) => {
          const msg = JSON.parse(ev.data)
          if (msg.id === id) {
            resolve(msg.error ? msg.error.message : null)
          }
        }
        ws.send(
          JSON.stringify({
            jsonrpc: '2.0',
            method: 'control.subscribe',
            params: { topic: 'vad.out' },
            id,
          }),
        )
      })
    },
    { url: WS_URL },
  )
  expect(err).toMatch(/not announced/i)
})

test('control.publish accepts an aux-port write', async ({ page }) => {
  // We need a session on the server to publish against. The test
  // manifest doesn't have an LFM2 node, but control.publish goes
  // through the router independent of the receiver — an unrecognised
  // aux port on `accumulator` still returns published:true because
  // the router accepts the packet and the node ignores it.
  //
  // The catch: sessions are only created *after* peer.offer
  // (ServerPeer::handle_offer → executor.create_session). A bare
  // WebSocket client that only announces never has a session, so
  // control.publish will error with "peer: session not yet created".
  //
  // We drive the full flow via the SPA instead — open the app, wait
  // for connected, then reach into its control client to publish.
  await page.addInitScript(() => {
    ;(window as unknown as { __TEST__: boolean }).__TEST__ = true
  })
  await page.goto(`/?ws=${encodeURIComponent(WS_URL)}`)
  await page.getByRole('button', { name: /start mic/i }).click()
  await expect(page.getByText(/^live$/)).toBeVisible({ timeout: 30_000 })

  // Type into the knowledge textarea and hit inject. The topic we
  // publish to on the test manifest is `accumulator.in.context` — the
  // router will accept and fan out; no handler needed for success.
  // We reroute by calling the exposed store publish hook via the
  // session singleton — but the simplest assertion here is that the
  // inject button becomes enabled once status goes live, and the
  // knowledge list grows by one.
  const textarea = page
    .getByPlaceholder(/the user's name is mathieu/i)
    .first()
  await textarea.fill('fact: pytest loves stubs')
  const injectBtn = page.getByRole('button', { name: /^inject$/i })
  await expect(injectBtn).toBeEnabled()
  await injectBtn.click()

  // Injected item shows up in the log (server round-trip succeeded;
  // on failure the log stays empty and an error pill appears).
  await expect(page.getByText(/fact: pytest loves stubs/)).toBeVisible()
  // No error banner.
  await expect(page.locator('[role="alert"]')).toHaveCount(0)
})

test('control.subscribe forwards tap events from the pipeline', async ({
  page,
}) => {
  // Full SPA path: open, connect, then subscribe to `resample_in.out`
  // (the first node that always produces output — it just downsamples
  // the incoming mic audio). Taps fire per produced frame, so even
  // in a quiet 5s window we should see dozens of events.
  //
  // We go through `window.__session.control` rather than calling WS
  // directly so the subscription shares the SPA's session / WS.
  await page.addInitScript(() => {
    ;(window as unknown as { __TEST__: boolean }).__TEST__ = true
  })
  await page.goto(`/?ws=${encodeURIComponent(WS_URL)}`)
  await page.getByRole('button', { name: /start mic/i }).click()
  await expect(page.getByText(/^live$/)).toBeVisible({ timeout: 30_000 })

  // Install a test-local subscription counter on the SPA's session,
  // then subscribe and wait for events.
  await page.evaluate(async () => {
    const session = (
      window as unknown as {
        __session?: {
          control: {
            subscribe: (
              topic: string,
              h: (ev: unknown) => void,
            ) => Promise<() => void>
          }
        }
      }
    ).__session
    if (!session) throw new Error('session bridge missing')
    ;(window as unknown as { __tapCount: number }).__tapCount = 0
    await session.control.subscribe('resample_in.out', () => {
      ;(window as unknown as { __tapCount: number }).__tapCount += 1
    })
  })

  await expect
    .poll(
      async () =>
        await page.evaluate(
          () =>
            (window as unknown as { __tapCount: number }).__tapCount ?? 0,
        ),
      {
        timeout: 15_000,
        message: 'no tap events received from resample_in.out',
      },
    )
    .toBeGreaterThan(0)
})
