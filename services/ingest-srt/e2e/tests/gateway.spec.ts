import { test, expect } from '@playwright/test';

test.describe('SRT Ingest Gateway E2E', () => {

  test.describe('Health and Metrics', () => {
    test('health endpoint returns OK', async ({ request }) => {
      const response = await request.get('/health');
      expect(response.ok()).toBeTruthy();
      expect(await response.text()).toBe('OK');
    });

    test('metrics endpoint returns valid JSON', async ({ request }) => {
      const response = await request.get('/metrics');
      expect(response.ok()).toBeTruthy();

      const metrics = await response.json();
      expect(metrics).toHaveProperty('sessions_created');
      expect(metrics).toHaveProperty('sessions_ended');
      expect(metrics).toHaveProperty('active_sessions');
      expect(metrics).toHaveProperty('webhook_attempts');
      expect(metrics).toHaveProperty('webhook_successes');
      expect(metrics).toHaveProperty('webhook_failures');
    });
  });

  test.describe('Demo UI', () => {
    test('loads demo page', async ({ page }) => {
      await page.goto('/');

      // Check page title
      await expect(page).toHaveTitle(/SRT Ingest Gateway/);

      // Check main elements are present
      await expect(page.locator('h1')).toContainText('SRT Ingest Gateway');
      await expect(page.locator('#createBtn')).toBeVisible();
      await expect(page.locator('#pipeline')).toBeVisible();
    });

    test('shows metrics on load', async ({ page }) => {
      await page.goto('/');

      // Wait for metrics to load (polls every 5s, but should load on init too)
      await expect(page.locator('#activeSessions')).toBeVisible({ timeout: 10000 });

      // Metrics should show numbers
      const activeSessionsText = await page.locator('#activeSessions').textContent();
      expect(activeSessionsText).toMatch(/\d+/);
    });

    test('can select different pipelines', async ({ page }) => {
      await page.goto('/');

      const pipelineSelect = page.locator('#pipeline');
      await expect(pipelineSelect).toBeVisible();

      // Check available options
      const options = await pipelineSelect.locator('option').allTextContents();
      expect(options.length).toBeGreaterThan(0);
      expect(options).toContain('Audio Quality Analysis');
    });
  });

  test.describe('Session API', () => {
    test('creates a session via API', async ({ request }) => {
      const response = await request.post('/api/ingest/sessions', {
        data: {
          pipeline: 'demo_audio_quality_v1',
          audio_enabled: true,
          video_enabled: false
        }
      });

      expect(response.ok()).toBeTruthy();
      expect(response.status()).toBe(201);

      const session = await response.json();
      expect(session).toHaveProperty('session_id');
      expect(session).toHaveProperty('srt_url');
      expect(session).toHaveProperty('ffmpeg_command_copy');
      expect(session).toHaveProperty('ffmpeg_command_transcode');
      expect(session).toHaveProperty('events_url');
      expect(session).toHaveProperty('expires_at');
      expect(session.session_id).toMatch(/^sess_/);
      expect(session.srt_url).toContain('srt://');
    });

    test('retrieves session status', async ({ request }) => {
      // Create a session first
      const createResponse = await request.post('/api/ingest/sessions', {
        data: {
          pipeline: 'demo_audio_quality_v1',
          audio_enabled: true,
          video_enabled: false
        }
      });
      const session = await createResponse.json();

      // Get session status
      const statusResponse = await request.get(`/api/ingest/sessions/${session.session_id}`);
      expect(statusResponse.ok()).toBeTruthy();

      const status = await statusResponse.json();
      expect(status).toHaveProperty('session_id', session.session_id);
      expect(status).toHaveProperty('state');
      expect(status).toHaveProperty('pipeline');
      expect(status).toHaveProperty('created_at');
      expect(['created', 'connected', 'streaming', 'ended']).toContain(status.state);
    });

    test('deletes a session', async ({ request }) => {
      // Create a session
      const createResponse = await request.post('/api/ingest/sessions', {
        data: {
          pipeline: 'demo_audio_quality_v1',
          audio_enabled: true,
          video_enabled: false
        }
      });
      const session = await createResponse.json();

      // Delete it
      const deleteResponse = await request.delete(`/api/ingest/sessions/${session.session_id}`);
      expect(deleteResponse.status()).toBe(204);

      // Verify it's now in ended state (sessions are kept with ended state)
      const statusResponse = await request.get(`/api/ingest/sessions/${session.session_id}`);
      expect(statusResponse.ok()).toBeTruthy();
      const status = await statusResponse.json();
      expect(status.state).toBe('ended');
    });

    test('returns 404 for non-existent session', async ({ request }) => {
      const response = await request.get('/api/ingest/sessions/sess_nonexistent');
      expect(response.status()).toBe(404);
    });

    test('accepts any pipeline name', async ({ request }) => {
      // The API should accept any pipeline name (it doesn't validate against a list)
      const response = await request.post('/api/ingest/sessions', {
        data: {
          pipeline: 'any_pipeline_name',
          audio_enabled: true,
          video_enabled: false
        }
      });

      expect(response.status()).toBe(201);
    });
  });

  test.describe('Session Creation via UI', () => {
    test('creates session from form', async ({ page }) => {
      await page.goto('/');

      // Select pipeline
      await page.selectOption('#pipeline', 'demo_audio_quality_v1');

      // Click create session
      await page.click('#createBtn');

      // Wait for session info to appear
      await expect(page.locator('#sessionInfo')).toBeVisible({ timeout: 10000 });

      // Check session ID is displayed
      const sessionIdText = await page.locator('#sessionId').textContent();
      expect(sessionIdText).toMatch(/sess_/);

      // Check FFmpeg command is shown
      const ffmpegCmd = await page.locator('#ffmpegCommand').textContent();
      expect(ffmpegCmd).toContain('ffmpeg');
      expect(ffmpegCmd).toContain('srt://');
    });

    test('copy button copies FFmpeg command', async ({ page, context }) => {
      // Grant clipboard permissions
      await context.grantPermissions(['clipboard-read', 'clipboard-write']);

      await page.goto('/');
      await page.selectOption('#pipeline', 'demo_audio_quality_v1');
      await page.click('#createBtn');

      await expect(page.locator('#sessionInfo')).toBeVisible({ timeout: 10000 });

      // Click copy button
      await page.click('.copy-btn');

      // Verify clipboard content
      const clipboardText = await page.evaluate(() => navigator.clipboard.readText());
      expect(clipboardText).toContain('ffmpeg');
    });

    test('end session button works', async ({ page }) => {
      await page.goto('/');
      await page.selectOption('#pipeline', 'demo_audio_quality_v1');
      await page.click('#createBtn');

      await expect(page.locator('#sessionInfo')).toBeVisible({ timeout: 10000 });

      // Click end session
      await page.click('.btn-danger');

      // Session info should be hidden
      await expect(page.locator('#sessionInfo')).toBeHidden({ timeout: 5000 });

      // Form should be visible again
      await expect(page.locator('#createSessionForm')).toBeVisible();
    });
  });

  test.describe('SSE Events', () => {
    test('SSE endpoint accepts connection', async ({ page, request }) => {
      // Create a session first
      const createResponse = await request.post('/api/ingest/sessions', {
        data: {
          pipeline: 'demo_audio_quality_v1',
          audio_enabled: true,
          video_enabled: false
        }
      });
      const session = await createResponse.json();

      // Navigate to page first so we have a context for fetch
      await page.goto('/');

      // For SSE, we can't use request.get() because it waits for completion
      // Instead, we'll use page.evaluate to create a quick EventSource connection test
      const sseTest = await page.evaluate(async (sessionId: string) => {
        return new Promise<{connected: boolean, contentType: string | null}>((resolve) => {
          const controller = new AbortController();
          const timeout = setTimeout(() => {
            controller.abort();
            resolve({ connected: false, contentType: null });
          }, 3000);

          fetch(`/api/ingest/sessions/${sessionId}/events`, {
            headers: { 'Accept': 'text/event-stream' },
            signal: controller.signal
          }).then(res => {
            clearTimeout(timeout);
            // We got headers - that's success for SSE
            resolve({
              connected: res.ok,
              contentType: res.headers.get('content-type')
            });
            controller.abort(); // Close the connection
          }).catch(() => {
            clearTimeout(timeout);
            resolve({ connected: false, contentType: null });
          });
        });
      }, session.session_id);

      expect(sseTest.connected).toBe(true);
      expect(sseTest.contentType).toContain('text/event-stream');
    });

    test('UI connects to SSE on session creation', async ({ page }) => {
      await page.goto('/');

      // Listen for SSE connection
      const ssePromise = page.waitForRequest(req =>
        req.url().includes('/events') && req.method() === 'GET',
        { timeout: 15000 }
      );

      await page.selectOption('#pipeline', 'demo_audio_quality_v1');
      await page.click('#createBtn');

      await expect(page.locator('#sessionInfo')).toBeVisible({ timeout: 10000 });

      // Verify SSE connection was made
      const sseRequest = await ssePromise;
      expect(sseRequest.url()).toContain('/api/ingest/sessions/');
      expect(sseRequest.url()).toContain('/events');
    });

    test('events container is visible', async ({ page }) => {
      await page.goto('/');
      await page.selectOption('#pipeline', 'demo_audio_quality_v1');
      await page.click('#createBtn');

      await expect(page.locator('#sessionInfo')).toBeVisible({ timeout: 10000 });

      // The event container should be visible
      await expect(page.locator('#eventsContainer')).toBeVisible();

      // Initially shows waiting message
      await expect(page.locator('#eventsContainer')).toContainText('Waiting for stream');
    });
  });

  test.describe('Error Handling', () => {
    test('UI shows error for failed requests', async ({ page }) => {
      await page.goto('/');

      // Intercept and fail the request
      await page.route('/api/ingest/sessions', route => {
        route.fulfill({
          status: 500,
          contentType: 'application/json',
          body: JSON.stringify({ error: 'Internal Server Error' })
        });
      });

      // Dismiss the alert dialog when it appears
      page.on('dialog', dialog => dialog.dismiss());

      await page.click('#createBtn');

      // Session info should not appear
      await expect(page.locator('#sessionInfo')).toBeHidden({ timeout: 5000 });
    });
  });

  test.describe('Metrics Update', () => {
    test('sessions created count increases after creation', async ({ request }) => {
      // Get initial metrics
      const initialResponse = await request.get('/metrics');
      const initialMetrics = await initialResponse.json();
      const initialCreated = initialMetrics.sessions_created;

      // Create a session via API
      await request.post('/api/ingest/sessions', {
        data: {
          pipeline: 'demo_audio_quality_v1',
          audio_enabled: true,
          video_enabled: false
        }
      });

      // Get updated metrics
      const newResponse = await request.get('/metrics');
      const newMetrics = await newResponse.json();
      const newCreated = newMetrics.sessions_created;

      expect(newCreated).toBeGreaterThan(initialCreated);
    });
  });

  test.describe('Full Session Lifecycle', () => {
    test('complete session flow: create -> view -> end', async ({ page }) => {
      await page.goto('/');

      // Step 1: Verify initial state
      await expect(page.locator('#sessionInfo')).toBeHidden();

      // Step 2: Create session
      await page.selectOption('#pipeline', 'demo_audio_quality_v1');
      await page.click('#createBtn');

      // Step 3: Verify session is active
      await expect(page.locator('#sessionInfo')).toBeVisible({ timeout: 10000 });
      const sessionId = await page.locator('#sessionId').textContent();
      expect(sessionId).toMatch(/sess_/);

      // Step 4: Verify SSE connection established
      const healthScore = page.locator('#healthScore');
      await expect(healthScore).toBeVisible();

      // Step 5: End session
      await page.click('.btn-danger');

      // Step 6: Verify cleanup
      await expect(page.locator('#sessionInfo')).toBeHidden({ timeout: 5000 });
      await expect(page.locator('#createSessionForm')).toBeVisible();
    });
  });
});

// Additional utility tests
test.describe('Static Assets', () => {
  test('serves index.html at root', async ({ request }) => {
    const response = await request.get('/');
    expect(response.ok()).toBeTruthy();
    expect(response.headers()['content-type']).toContain('text/html');
  });

  test('serves index.html for unknown routes (SPA fallback)', async ({ request }) => {
    const response = await request.get('/some/unknown/path');
    expect(response.ok()).toBeTruthy();
    expect(response.headers()['content-type']).toContain('text/html');
  });
});

// True E2E test: Create session -> Stream via SRT -> Receive events
test.describe('Full E2E Streaming', () => {
  // Path to test video file (relative to e2e/tests directory -> workspace root)
  const TEST_VIDEO = 'input.mp4';

  // Skip if ffmpeg is not available or doesn't have SRT support
  test.beforeAll(async () => {
    const { exec } = await import('child_process');
    const { promisify } = await import('util');
    const { existsSync } = await import('fs');
    const { resolve } = await import('path');
    const execAsync = promisify(exec);

    // Check if test video exists (e2e/tests -> e2e -> ingest-srt -> services -> remotemedia-sdk)
    const videoPath = resolve(__dirname, '../../../..', TEST_VIDEO);
    if (!existsSync(videoPath)) {
      console.log(`Test video not found at ${videoPath}, skipping E2E streaming tests`);
      test.skip();
      return;
    }

    try {
      // Check if ffmpeg has SRT support
      const { stdout } = await execAsync('ffmpeg -protocols 2>&1');
      if (!stdout.includes('srt')) {
        console.log('FFmpeg does not have SRT support, skipping E2E streaming tests');
        test.skip();
      }
    } catch {
      console.log('FFmpeg not available, skipping E2E streaming tests');
      test.skip();
    }
  });

  test('streams via SRT and receives health events', async ({ page, request }) => {
    const { spawn } = await import('child_process');
    const { resolve } = await import('path');

    // Resolve the test video path
    const videoPath = resolve(__dirname, '../../../..', TEST_VIDEO);

    // Step 1: Create a session via API
    const createResponse = await request.post('/api/ingest/sessions', {
      data: {
        pipeline: 'demo_audio_quality_v1',
        audio_enabled: true,
        video_enabled: true
      }
    });
    expect(createResponse.ok()).toBeTruthy();
    const session = await createResponse.json();
    const sessionId = session.session_id;
    const srtUrl = session.srt_url;

    console.log(`Created session: ${sessionId}`);
    console.log(`SRT URL: ${srtUrl}`);

    // Step 2: Navigate to page and set up SSE listener
    await page.goto('/');

    // Set up SSE event collection in the browser
    const eventsPromise = page.evaluate(async (sid: string) => {
      return new Promise<{ events: any[], error?: string }>((resolve) => {
        const events: any[] = [];
        const eventSource = new EventSource(`/api/ingest/sessions/${sid}/events`);

        const timeout = setTimeout(() => {
          eventSource.close();
          resolve({ events });
        }, 20000); // Wait up to 20 seconds for events

        // SSE events are sent with named event types (health, system, alert, event)
        // We need to listen for each type specifically
        const eventTypes = ['health', 'system', 'alert', 'event'];
        eventTypes.forEach(eventType => {
          eventSource.addEventListener(eventType, (event: MessageEvent) => {
            try {
              const data = JSON.parse(event.data);
              data.event_type = eventType; // Add event_type for filtering
              events.push(data);
              console.log(`Received ${eventType} event:`, data);
              // If we've received at least 3 health events, we're good
              if (events.filter(e => e.event_type === 'health').length >= 3) {
                clearTimeout(timeout);
                eventSource.close();
                resolve({ events });
              }
            } catch (e) {
              // Ignore parse errors for keep-alive messages
            }
          });
        });

        // Also listen for generic messages (keep-alive, etc.)
        eventSource.onmessage = (event) => {
          console.log(`Received generic message:`, event.data);
        };

        eventSource.onerror = () => {
          clearTimeout(timeout);
          eventSource.close();
          resolve({ events, error: 'SSE connection error' });
        };
      });
    }, sessionId);

    // Step 3: Start FFmpeg to stream input.mp4 via SRT
    // Stream 10 seconds of the file with -t 10
    const ffmpegArgs = [
      '-re',           // Read input at native frame rate
      '-t', '10',      // Stream 10 seconds for enough health events
      '-i', videoPath,
      '-c:v', 'libx264',
      '-preset', 'ultrafast',
      '-tune', 'zerolatency',
      '-g', '30',
      '-c:a', 'aac',
      '-ar', '48000',
      '-b:a', '128k',
      '-f', 'mpegts',
      srtUrl
    ];

    console.log('Starting FFmpeg with input:', videoPath);
    const ffmpeg = spawn('ffmpeg', ffmpegArgs, {
      stdio: ['pipe', 'pipe', 'pipe']
    });

    let ffmpegOutput = '';
    ffmpeg.stderr?.on('data', (data: Buffer) => {
      ffmpegOutput += data.toString();
    });

    // Wait for FFmpeg to finish or timeout
    const ffmpegPromise = new Promise<{ success: boolean, output: string }>((resolve) => {
      const timeout = setTimeout(() => {
        ffmpeg.kill('SIGTERM');
        resolve({ success: false, output: ffmpegOutput });
      }, 30000);

      ffmpeg.on('close', (code: number | null) => {
        clearTimeout(timeout);
        resolve({ success: code === 0, output: ffmpegOutput });
      });

      ffmpeg.on('error', (err: Error) => {
        clearTimeout(timeout);
        resolve({ success: false, output: err.message });
      });
    });

    // Step 4: Wait for both FFmpeg and SSE events
    const [ffmpegResult, sseResult] = await Promise.all([
      ffmpegPromise,
      eventsPromise
    ]);

    console.log('FFmpeg result:', ffmpegResult.success ? 'success' : 'failed');
    if (!ffmpegResult.success) {
      console.log('FFmpeg output:', ffmpegResult.output.slice(-500));
    }
    console.log('Events received:', sseResult.events.length);

    // Step 5: Verify session transitioned to streaming state
    const statusResponse = await request.get(`/api/ingest/sessions/${sessionId}`);
    const status = await statusResponse.json();
    console.log('Final session state:', status.state);

    // Step 6: Assertions
    // FFmpeg should have connected successfully
    expect(ffmpegResult.output).toContain('Output #0');

    // We should have received some events
    expect(sseResult.events.length).toBeGreaterThan(0);

    // At least one health event should have been received
    const healthEvents = sseResult.events.filter((e: any) => e.event_type === 'health');
    expect(healthEvents.length).toBeGreaterThan(0);

    // Session should have transitioned to streaming or ended
    expect(['streaming', 'ended']).toContain(status.state);

    // Clean up: end the session
    await request.delete(`/api/ingest/sessions/${sessionId}`);
  });

  test('session state transitions during streaming', async ({ request }) => {
    const { spawn } = await import('child_process');
    const { resolve } = await import('path');

    const videoPath = resolve(__dirname, '../../../..', TEST_VIDEO);

    // Create session
    const createResponse = await request.post('/api/ingest/sessions', {
      data: {
        pipeline: 'demo_audio_quality_v1',
        audio_enabled: true,
        video_enabled: true
      }
    });
    const session = await createResponse.json();
    const sessionId = session.session_id;

    // Verify initial state is 'created'
    let statusResponse = await request.get(`/api/ingest/sessions/${sessionId}`);
    let status = await statusResponse.json();
    expect(status.state).toBe('created');

    // Start FFmpeg streaming (short 2-second stream)
    const ffmpegArgs = [
      '-re',
      '-t', '2',
      '-i', videoPath,
      '-c', 'copy',
      '-f', 'mpegts',
      session.srt_url
    ];

    const ffmpeg = spawn('ffmpeg', ffmpegArgs, { stdio: 'pipe' });

    // Wait a moment for connection
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Check state transitioned to connected or streaming
    statusResponse = await request.get(`/api/ingest/sessions/${sessionId}`);
    status = await statusResponse.json();
    expect(['connected', 'streaming']).toContain(status.state);

    // Wait for FFmpeg to complete
    await new Promise<void>((resolve) => {
      ffmpeg.on('close', () => resolve());
      setTimeout(() => {
        ffmpeg.kill('SIGTERM');
        resolve();
      }, 10000);
    });

    // Give the server a moment to process the disconnect
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Session should now be ended (client disconnected)
    statusResponse = await request.get(`/api/ingest/sessions/${sessionId}`);
    status = await statusResponse.json();
    expect(status.state).toBe('ended');

    // Clean up
    await request.delete(`/api/ingest/sessions/${sessionId}`);
  });

  test('metrics update during streaming', async ({ request }) => {
    const { spawn } = await import('child_process');
    const { resolve } = await import('path');

    const videoPath = resolve(__dirname, '../../../..', TEST_VIDEO);

    // Get initial metrics
    let metricsResponse = await request.get('/metrics');
    const initialMetrics = await metricsResponse.json();

    // Create session and stream
    const createResponse = await request.post('/api/ingest/sessions', {
      data: {
        pipeline: 'demo_audio_quality_v1',
        audio_enabled: true,
        video_enabled: true
      }
    });
    const session = await createResponse.json();

    // Check sessions_created increased
    metricsResponse = await request.get('/metrics');
    let metrics = await metricsResponse.json();
    expect(metrics.sessions_created).toBeGreaterThan(initialMetrics.sessions_created);
    expect(metrics.active_sessions).toBeGreaterThanOrEqual(1);

    // Stream briefly (1 second)
    const ffmpegArgs = [
      '-re',
      '-t', '1',
      '-i', videoPath,
      '-c', 'copy',
      '-f', 'mpegts',
      session.srt_url
    ];

    const ffmpeg = spawn('ffmpeg', ffmpegArgs, { stdio: 'pipe' });
    await new Promise<void>((resolve) => {
      ffmpeg.on('close', () => resolve());
      setTimeout(() => {
        ffmpeg.kill('SIGTERM');
        resolve();
      }, 10000);
    });

    // Wait for session to end
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Check sessions_ended increased
    metricsResponse = await request.get('/metrics');
    metrics = await metricsResponse.json();
    expect(metrics.sessions_ended).toBeGreaterThan(initialMetrics.sessions_ended);

    // Clean up
    await request.delete(`/api/ingest/sessions/${session.session_id}`);
  });
});
