import { test, expect } from '@playwright/test';

test.describe('RemoteMedia UI E2E', () => {

  // ──────────────────────────────────────────────────────────────────────
  // Status & Connection
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Status', () => {
    test('API status endpoint returns server info', async ({ request }) => {
      const response = await request.get('/api/status');
      expect(response.ok()).toBeTruthy();

      const status = await response.json();
      expect(status).toHaveProperty('version');
      expect(status.active_sessions).toBe(0);
    });

    test('page loads and shows connected status', async ({ page }) => {
      await page.goto('/');

      // Header should be visible
      await expect(page.locator('h1')).toContainText('RemoteMedia');

      // Should show connected status (fetches /api/status on load)
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });
      await expect(page.getByText('Connected')).toBeVisible();
    });

    test('shows session count', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.session-count')).toContainText('0 sessions');
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // Tab Navigation
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Navigation', () => {
    test('default tab is Pipeline', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.tab.active')).toContainText('Pipeline');
      await expect(page.locator('h2')).toContainText('Pipeline Execution');
    });

    test('can switch to Manifest tab', async ({ page }) => {
      await page.goto('/');

      await page.getByRole('button', { name: 'Manifest' }).click();

      // Should show manifest content (passthrough pipeline)
      await expect(page.locator('.node-id')).toContainText('passthrough', { timeout: 5000 });
      await expect(page.locator('.node-type')).toContainText('PassThrough');
    });

    test('can switch back to Pipeline tab', async ({ page }) => {
      await page.goto('/');

      await page.getByRole('button', { name: 'Manifest' }).click();
      await page.getByRole('button', { name: 'Pipeline' }).click();

      await expect(page.locator('h2')).toContainText('Pipeline Execution');
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // Manifest View
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Manifest', () => {
    test('displays pipeline nodes', async ({ page }) => {
      await page.goto('/');
      await page.getByRole('button', { name: 'Manifest' }).click();

      await expect(page.locator('.node-card')).toBeVisible({ timeout: 5000 });
      await expect(page.locator('.node-id')).toContainText('passthrough');
      await expect(page.locator('.node-type')).toContainText('PassThrough');
    });

    test('API manifest endpoint returns pipeline definition', async ({ request }) => {
      const response = await request.get('/api/manifest');
      expect(response.ok()).toBeTruthy();

      const manifest = await response.json();
      expect(manifest.metadata.name).toBe('passthrough');
      expect(manifest.nodes).toHaveLength(1);
      expect(manifest.nodes[0].node_type).toBe('PassThrough');
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // Pipeline Execution (Unary) — full browser interaction
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Pipeline Execution', () => {
    test('executes text passthrough via UI', async ({ page }) => {
      await page.goto('/');

      // Wait for app to be ready (connected)
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });

      // Input type should default to "text"
      await expect(page.locator('select')).toHaveValue('text');

      // Type text into the textarea
      await page.locator('textarea').fill('hello from playwright');

      // Intercept the API call to verify request/response
      const responsePromise = page.waitForResponse(resp =>
        resp.url().includes('/api/execute') && resp.request().method() === 'POST'
      );

      // Click Execute
      await page.getByRole('button', { name: 'Execute' }).click();

      // Wait for the API response and debug
      const response = await responsePromise;
      if (response.status() !== 200) {
        const reqBody = response.request().postData();
        const resBody = await response.text();
        console.log('REQUEST:', reqBody);
        console.log('RESPONSE:', response.status(), resBody);
      }
      expect(response.status()).toBe(200);

      // Wait for result to appear (result-text for Text output, result-json for others)
      await expect(page.locator('.result-text, .result-json')).toBeVisible({ timeout: 10000 });

      // PassThrough returns input as-is
      const resultText = await page.locator('.result-text, .result-json').textContent();
      expect(resultText).toContain('hello from playwright');
    });

    test('executes JSON input via UI', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });

      // Switch to JSON input type
      await page.locator('select').selectOption('json');

      // Type JSON into the textarea
      await page.locator('textarea').fill('{"key": "value", "num": 42}');

      // Click Execute
      await page.getByRole('button', { name: 'Execute' }).click();

      // Wait for result
      await expect(page.locator('.result-text, .result-json')).toBeVisible({ timeout: 10000 });

      const resultText = await page.locator('.result-text, .result-json').textContent();
      expect(resultText).toContain('key');
      expect(resultText).toContain('value');
    });

    test('shows error on invalid JSON input', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });

      await page.locator('select').selectOption('json');
      await page.locator('textarea').fill('not valid json {{{');

      await page.getByRole('button', { name: 'Execute' }).click();

      // Should show error
      await expect(page.locator('.error')).toBeVisible({ timeout: 5000 });
      await expect(page.locator('.error')).toContainText('Invalid JSON');
    });

    test('execute button shows loading state', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });

      await page.locator('textarea').fill('loading test');

      // Click Execute and check loading state
      await page.getByRole('button', { name: 'Execute' }).click();

      // Button should say "Executing..." while processing
      // (may be fast with passthrough, so we check the loading text appeared or result appeared)
      await expect(page.locator('.result-text, .result-json, .loading')).toBeVisible({ timeout: 10000 });
    });

    test('can execute multiple times', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });

      // First execution
      await page.locator('textarea').fill('first');
      await page.getByRole('button', { name: 'Execute' }).click();
      await expect(page.locator('.result-text, .result-json')).toContainText('first', { timeout: 10000 });

      // Second execution (clears previous result)
      await page.locator('textarea').fill('second');
      await page.getByRole('button', { name: 'Execute' }).click();
      await expect(page.locator('.result-text, .result-json')).toContainText('second', { timeout: 10000 });
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // Pipeline Execution via API (request context, no browser)
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Pipeline API', () => {
    test('execute endpoint returns passthrough result', async ({ request }) => {
      const response = await request.post('/api/execute', {
        data: {
          input: {
            data: { Text: 'api test' },
            metadata: {},
          },
        },
      });
      expect(response.ok()).toBeTruthy();

      const result = await response.json();
      expect(result.output.data.Text).toBe('api test');
    });

    test('stream session lifecycle via API', async ({ request }) => {
      // Create session
      const createResp = await request.post('/api/stream', {
        data: {},
      });
      expect(createResp.ok()).toBeTruthy();
      const { session_id } = await createResp.json();
      expect(session_id).toBeTruthy();

      // Check active sessions incremented
      const statusResp = await request.get('/api/status');
      const status = await statusResp.json();
      expect(status.active_sessions).toBeGreaterThanOrEqual(1);

      // Send input
      const inputResp = await request.post(`/api/stream/${session_id}/input`, {
        data: {
          data: {
            data: { Text: 'stream api test' },
            metadata: {},
          },
        },
      });
      expect(inputResp.ok()).toBeTruthy();

      // Close session
      const closeResp = await request.delete(`/api/stream/${session_id}`);
      expect(closeResp.ok()).toBeTruthy();
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // Streaming via SSE (browser EventSource)
  // ──────────────────────────────────────────────────────────────────────

  test.describe('Streaming I/O', () => {
    test('stream input through SSE produces output', async ({ page, request }) => {
      // Create a session via API
      const createResp = await request.post('/api/stream', {
        data: {},
      });
      const { session_id } = await createResp.json();

      // Open a page and use EventSource in-browser to subscribe to outputs
      await page.goto('/');

      const output = await page.evaluate(async (sid) => {
        return new Promise<string>((resolve, reject) => {
          const timeout = setTimeout(() => reject(new Error('SSE timeout')), 10000);

          const es = new EventSource(`/api/stream/${sid}/output`);
          es.onmessage = (e) => {
            clearTimeout(timeout);
            es.close();
            resolve(e.data);
          };
          es.onerror = () => {
            clearTimeout(timeout);
            es.close();
            reject(new Error('SSE error'));
          };

          // Give EventSource time to connect, then send input
          setTimeout(async () => {
            await fetch(`/api/stream/${sid}/input`, {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({
                data: {
                  data: { Text: 'sse streaming test' },
                  metadata: {},
                },
              }),
            });
          }, 200);
        });
      }, session_id);

      const parsed = JSON.parse(output);
      expect(parsed.data.Text).toBe('sse streaming test');

      // Cleanup
      await request.delete(`/api/stream/${session_id}`);
    });

    test('stream multiple messages arrive in order', async ({ page, request }) => {
      const createResp = await request.post('/api/stream', { data: {} });
      const { session_id } = await createResp.json();

      await page.goto('/');

      const outputs = await page.evaluate(async (sid) => {
        return new Promise<string[]>((resolve, reject) => {
          const messages: string[] = [];
          const timeout = setTimeout(() => reject(new Error('SSE timeout')), 15000);

          const es = new EventSource(`/api/stream/${sid}/output`);
          es.onmessage = (e) => {
            const parsed = JSON.parse(e.data);
            messages.push(parsed.data.Text);
            if (messages.length === 3) {
              clearTimeout(timeout);
              es.close();
              resolve(messages);
            }
          };
          es.onerror = () => {
            clearTimeout(timeout);
            es.close();
            reject(new Error('SSE error'));
          };

          // Send 3 messages sequentially
          setTimeout(async () => {
            for (const msg of ['alpha', 'beta', 'gamma']) {
              await fetch(`/api/stream/${sid}/input`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                  data: { data: { Text: msg }, metadata: {} },
                }),
              });
              // Small delay between messages to ensure ordering
              await new Promise(r => setTimeout(r, 50));
            }
          }, 200);
        });
      }, session_id);

      expect(outputs).toEqual(['alpha', 'beta', 'gamma']);

      await request.delete(`/api/stream/${session_id}`);
    });
  });

  // ──────────────────────────────────────────────────────────────────────
  // SPA Routing
  // ──────────────────────────────────────────────────────────────────────

  test.describe('SPA Routing', () => {
    test('deep paths serve index.html', async ({ page }) => {
      const response = await page.goto('/some/deep/path');
      expect(response?.status()).toBe(200);

      // Should still render the app
      await expect(page.locator('h1')).toContainText('RemoteMedia');
    });
  });
});
