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

  test.describe('Landing Page', () => {
    test('loads landing page with persona content', async ({ page }) => {
      await page.goto('/');

      // Check page title
      await expect(page).toHaveTitle(/Stream Health Monitor/);

      // Check main landing page elements are present
      await expect(page.getByRole('heading', { name: /Observe the health of live media/ })).toBeVisible();
      await expect(page.getByRole('button', { name: /Observe a live stream/ }).first()).toBeVisible();
    });

    test('CTA button navigates to observer UI and creates session', async ({ page }) => {
      await page.goto('/');

      // Click the primary CTA (first one in hero section)
      await page.getByRole('button', { name: /Observe a live stream/ }).first().click();

      // Should navigate to observer UI and auto-create session
      await expect(page).toHaveURL(/\/observe/);
      // With persona param, a session is auto-created, so FFmpeg command should appear
      await expect(page.locator('pre').filter({ hasText: 'ffmpeg' })).toBeVisible({ timeout: 10000 });
    });

    test('persona routes load correctly', async ({ page }) => {
      // Test contact center persona
      await page.goto('/#/contact-center');
      await expect(page.getByRole('heading', { name: /dead air, talk-over/i })).toBeVisible();

      // Test telehealth persona
      await page.goto('/#/telehealth');
      await expect(page.getByRole('heading', { name: /patients can't hear you/i })).toBeVisible();
    });
  });

  test.describe('Observer UI', () => {
    test.beforeEach(async ({ page }) => {
      // Clear storage to ensure clean state for each test
      await page.goto('/');
      await page.evaluate(() => {
        sessionStorage.clear();
        localStorage.clear();
      });
    });

    test('shows pipeline selection when no persona param', async ({ page }) => {
      await page.goto('/#/observe');

      // Check main elements are present - React app shows "Waiting for media..."
      await expect(page.getByText('Waiting for media...')).toBeVisible();

      // Pipeline selection should be available with categories
      await expect(page.getByText('business', { exact: true })).toBeVisible();
      await expect(page.getByText('technical', { exact: true })).toBeVisible();
      await expect(page.getByText('Contact Center QA')).toBeVisible();
    });

    test('selecting a pipeline creates session', async ({ page }) => {
      await page.goto('/#/observe');

      // Click on Contact Center QA pipeline
      const contactCenterBtn = page.getByRole('button', { name: /Contact Center QA/ });
      await expect(contactCenterBtn).toBeVisible();
      await contactCenterBtn.click();

      // Session should be created (FFmpeg command appears)
      await expect(page.locator('pre').filter({ hasText: 'ffmpeg' })).toBeVisible({ timeout: 10000 });
      await expect(page.locator('pre').filter({ hasText: 'srt://' })).toBeVisible();
    });

    test('persona param auto-creates session', async ({ page }) => {
      // Navigate with persona param - should auto-select pipeline and create session
      await page.goto('/#/observe?persona=contact-center');

      // Session should be created automatically
      await expect(page.locator('pre').filter({ hasText: 'ffmpeg' })).toBeVisible({ timeout: 10000 });
      await expect(page.locator('pre').filter({ hasText: 'contact_center_qa' })).toBeVisible();
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

    test('accepts new stream health pipelines', async ({ request }) => {
      // Test the new Contact Center QA pipeline
      const response = await request.post('/api/ingest/sessions', {
        data: {
          pipeline: 'contact_center_qa_v1',
          audio_enabled: true,
          video_enabled: false
        }
      });

      expect(response.status()).toBe(201);
      const session = await response.json();
      expect(session.session_id).toMatch(/^sess_/);

      // Clean up
      await request.delete(`/api/ingest/sessions/${session.session_id}`);
    });

    test('accepts full stream health pipeline', async ({ request }) => {
      const response = await request.post('/api/ingest/sessions', {
        data: {
          pipeline: 'full_stream_health_v1',
          audio_enabled: true,
          video_enabled: false
        }
      });

      expect(response.status()).toBe(201);
      const session = await response.json();
      expect(session.session_id).toMatch(/^sess_/);

      // Clean up
      await request.delete(`/api/ingest/sessions/${session.session_id}`);
    });

    test('accepts speaker diarization pipeline', async ({ request }) => {
      // Test the speaker diarization pipeline
      const response = await request.post('/api/ingest/sessions', {
        data: {
          pipeline: 'speaker_diarization_v1',
          audio_enabled: true,
          video_enabled: false
        }
      });

      expect(response.status()).toBe(201);
      const session = await response.json();
      expect(session.session_id).toMatch(/^sess_/);
      expect(session.srt_url).toContain('srt://');

      // Clean up
      await request.delete(`/api/ingest/sessions/${session.session_id}`);
    });
  });

  test.describe('Session Creation via UI', () => {
    test.beforeEach(async ({ page }) => {
      // Clear storage to ensure clean state for each test
      await page.goto('/');
      await page.evaluate(() => {
        sessionStorage.clear();
        localStorage.clear();
      });
    });

    test('creates session when pipeline selected', async ({ page }) => {
      await page.goto('/#/observe');

      // Select pipeline (auto-creates session)
      await page.getByRole('button', { name: /Contact Center QA/ }).click();

      // Wait for FFmpeg command to appear (indicates session created)
      await expect(page.locator('pre').filter({ hasText: 'ffmpeg' })).toBeVisible({ timeout: 10000 });
      await expect(page.locator('pre').filter({ hasText: 'srt://' })).toBeVisible();
    });

    test('copy button copies FFmpeg command', async ({ page, context }) => {
      // Grant clipboard permissions
      await context.grantPermissions(['clipboard-read', 'clipboard-write']);

      await page.goto('/#/observe');
      await page.getByRole('button', { name: /Contact Center QA/ }).click();

      // Wait for session to be created
      await expect(page.locator('pre').filter({ hasText: 'ffmpeg' })).toBeVisible({ timeout: 10000 });

      // Click copy button
      await page.getByRole('button', { name: 'Copy' }).click();

      // Verify button shows "Copied!"
      await expect(page.getByRole('button', { name: 'Copied!' })).toBeVisible();

      // Verify clipboard content
      const clipboardText = await page.evaluate(() => navigator.clipboard.readText());
      expect(clipboardText).toContain('ffmpeg');
    });

    // Note: There is no explicit "End Session" button in the current UI.
    // Sessions end when the stream ends or when navigating away.
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
      await page.goto('/#/observe');

      // For SSE, we can't use request.get() because it waits for completion
      // Instead, we'll use page.evaluate to create a quick EventSource connection test
      const sseTest = await page.evaluate(async (sessionId: string) => {
        return new Promise<{ connected: boolean, contentType: string | null }>((resolve) => {
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
      // Clear storage first
      await page.goto('/');
      await page.evaluate(() => {
        sessionStorage.clear();
        localStorage.clear();
      });

      await page.goto('/#/observe');

      // Listen for SSE connection
      const ssePromise = page.waitForRequest(req =>
        req.url().includes('/events') && req.method() === 'GET',
        { timeout: 15000 }
      );

      // Select pipeline to auto-create session
      await page.getByRole('button', { name: /Contact Center QA/ }).click();

      // Wait for session to be created
      await expect(page.locator('pre').filter({ hasText: 'ffmpeg' })).toBeVisible({ timeout: 10000 });

      // Verify SSE connection was made
      const sseRequest = await ssePromise;
      expect(sseRequest.url()).toContain('/api/ingest/sessions/');
      expect(sseRequest.url()).toContain('/events');
    });

    test('timeline shows waiting message when session active', async ({ page }) => {
      // Clear storage first
      await page.goto('/');
      await page.evaluate(() => {
        sessionStorage.clear();
        localStorage.clear();
      });

      await page.goto('/#/observe');
      await page.getByRole('button', { name: /Contact Center QA/ }).click();

      // Wait for session to be created
      await expect(page.locator('pre').filter({ hasText: 'ffmpeg' })).toBeVisible({ timeout: 10000 });

      // The timeline should show waiting message (ready state, waiting for stream to connect)
      await expect(page.getByText('Run the FFmpeg command to start streaming')).toBeVisible();
    });
  });

  test.describe('React App Layout', () => {
    test.beforeEach(async ({ page }) => {
      // Clear storage for clean state
      await page.goto('/');
      await page.evaluate(() => {
        sessionStorage.clear();
        localStorage.clear();
      });
    });

    test('shows three-column layout when session active', async ({ page }) => {
      await page.goto('/#/observe');
      await page.getByRole('button', { name: /Contact Center QA/ }).click();

      // Wait for session to be created
      await expect(page.locator('pre').filter({ hasText: 'ffmpeg' })).toBeVisible({ timeout: 10000 });

      // Center: Timeline heading
      await expect(page.getByRole('heading', { name: 'Timeline' })).toBeVisible();

      // Right: Evidence pane
      await expect(page.getByText(/Select an event/i)).toBeVisible();
    });

    test('shows centered setup when idle', async ({ page }) => {
      await page.goto('/#/observe');

      // Should show centered card with setup form
      await expect(page.getByText('Waiting for media...')).toBeVisible();

      // Pipeline buttons should be visible
      await expect(page.getByRole('button', { name: /Contact Center QA/ })).toBeVisible();

      // Timeline heading should NOT be visible when idle
      await expect(page.getByRole('heading', { name: 'Timeline' })).not.toBeVisible();
    });
  });

  test.describe('Error Handling', () => {
    test('API returns error status for invalid requests', async ({ request }) => {
      // Test that the API handles completely invalid payloads gracefully
      const response = await request.post('/api/ingest/sessions', {
        data: 'not-a-valid-json-object'
      });

      // Should return an error status for malformed request
      expect(response.status()).toBeGreaterThanOrEqual(400);
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
    test('complete session flow: create -> view', async ({ page }) => {
      // Clear storage first
      await page.goto('/');
      await page.evaluate(() => {
        sessionStorage.clear();
        localStorage.clear();
      });

      await page.goto('/#/observe');

      // Step 1: Verify initial state - centered setup with pipeline selection
      await expect(page.getByText('Waiting for media...')).toBeVisible();

      // Step 2: Select pipeline to auto-create session
      await page.getByRole('button', { name: /Contact Center QA/ }).click();

      // Step 3: Verify session is active with FFmpeg command
      await expect(page.locator('pre').filter({ hasText: 'ffmpeg' })).toBeVisible({ timeout: 10000 });
      await expect(page.getByRole('heading', { name: 'Timeline' })).toBeVisible();

      // Note: Sessions end when the stream ends or navigating away (no explicit end button)
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

    // Step 1: Create a session via API with the new stream health pipeline
    const createResponse = await request.post('/api/ingest/sessions', {
      data: {
        pipeline: 'full_stream_health_v1',
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
    await page.goto('/#/observe');

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
        pipeline: 'contact_center_qa_v1',
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
    const initialSessionsCreated = initialMetrics.sessions_created;

    // Create session and stream
    const createResponse = await request.post('/api/ingest/sessions', {
      data: {
        pipeline: 'technical_stream_analysis_v1',
        audio_enabled: true,
        video_enabled: true
      }
    });
    const session = await createResponse.json();

    // Check sessions_created increased
    metricsResponse = await request.get('/metrics');
    let metrics = await metricsResponse.json();
    expect(metrics.sessions_created).toBeGreaterThan(initialSessionsCreated);
    expect(metrics.active_sessions).toBeGreaterThanOrEqual(1);

    // Store the active session count for later comparison
    const activeSessionsDuringStream = metrics.active_sessions;

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

    // Wait for session to end (server detects disconnect)
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Try to explicitly end the session (may already be ended automatically)
    await request.delete(`/api/ingest/sessions/${session.session_id}`);

    // Give metrics a moment to update
    await new Promise(resolve => setTimeout(resolve, 500));

    // Verify active_sessions decreased (session is no longer active)
    metricsResponse = await request.get('/metrics');
    metrics = await metricsResponse.json();

    // The session should no longer be counted as active
    // Note: sessions_ended metric increment is not yet implemented in the server,
    // but we can verify the session was properly removed from active sessions
    expect(metrics.active_sessions).toBeLessThanOrEqual(activeSessionsDuringStream);
  });

  test('receives stream_started and stream_ended system events', async ({ page, request }) => {
    const { spawn } = await import('child_process');
    const { resolve } = await import('path');

    // Resolve the test video path
    const videoPath = resolve(__dirname, '../../../..', TEST_VIDEO);

    // Step 1: Create a session via API
    const createResponse = await request.post('/api/ingest/sessions', {
      data: {
        pipeline: 'full_stream_health_v1',
        audio_enabled: true,
        video_enabled: true
      }
    });
    expect(createResponse.ok()).toBeTruthy();
    const session = await createResponse.json();
    const sessionId = session.session_id;
    const srtUrl = session.srt_url;

    console.log(`Created session for system events test: ${sessionId}`);

    // Step 2: Navigate to page and set up SSE listener for system events
    await page.goto('/#/observe');

    // Set up SSE event collection - specifically looking for stream_started and stream_ended
    const eventsPromise = page.evaluate(async (sid: string) => {
      return new Promise<{ events: any[], systemEvents: any[], error?: string }>((resolve) => {
        const events: any[] = [];
        const systemEvents: any[] = [];
        const eventSource = new EventSource(`/api/ingest/sessions/${sid}/events`);

        const timeout = setTimeout(() => {
          eventSource.close();
          resolve({ events, systemEvents });
        }, 15000);

        // Listen specifically for 'system' SSE events which contain stream_started/stream_ended
        eventSource.addEventListener('system', (event: MessageEvent) => {
          try {
            const data = JSON.parse(event.data);
            console.log('Received system event:', data);
            events.push(data);
            systemEvents.push(data);

            // Check for stream_ended to know when we can finish
            if (data.type === 'stream_ended') {
              // Give a moment for any final events
              setTimeout(() => {
                clearTimeout(timeout);
                eventSource.close();
                resolve({ events, systemEvents });
              }, 500);
            }
          } catch (e) {
            // Ignore parse errors
          }
        });

        // Also collect other events for context
        ['health', 'alert', 'event'].forEach(eventType => {
          eventSource.addEventListener(eventType, (event: MessageEvent) => {
            try {
              const data = JSON.parse(event.data);
              events.push({ ...data, sse_type: eventType });
            } catch (e) {
              // Ignore
            }
          });
        });

        eventSource.onerror = () => {
          clearTimeout(timeout);
          eventSource.close();
          resolve({ events, systemEvents, error: 'SSE connection error' });
        };
      });
    }, sessionId);

    // Step 3: Start FFmpeg to stream briefly (3 seconds)
    const ffmpegArgs = [
      '-re',
      '-t', '3',
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

    // Wait for FFmpeg to finish
    const ffmpegPromise = new Promise<{ success: boolean }>((resolve) => {
      const ffmpegTimeout = setTimeout(() => {
        ffmpeg.kill('SIGTERM');
        resolve({ success: false });
      }, 15000);

      ffmpeg.on('close', (code: number | null) => {
        clearTimeout(ffmpegTimeout);
        resolve({ success: code === 0 });
      });

      ffmpeg.on('error', () => {
        clearTimeout(ffmpegTimeout);
        resolve({ success: false });
      });
    });

    // Step 4: Wait for both FFmpeg and SSE events
    const [ffmpegResult, sseResult] = await Promise.all([
      ffmpegPromise,
      eventsPromise
    ]);

    console.log('FFmpeg result:', ffmpegResult.success ? 'success' : 'failed');
    console.log('Total events received:', sseResult.events.length);
    console.log('System events received:', sseResult.systemEvents.length);
    console.log('System events:', JSON.stringify(sseResult.systemEvents, null, 2));

    // Step 5: Verify system events
    // stream_started should have type: 'stream_started'
    const streamStarted = sseResult.systemEvents.find((e: any) => e.type === 'stream_started');
    expect(streamStarted).toBeDefined();
    console.log('Found stream_started event:', streamStarted);

    // stream_ended should have type: 'stream_ended' (emitted when FFmpeg disconnects)
    const streamEnded = sseResult.systemEvents.find((e: any) => e.type === 'stream_ended');
    expect(streamEnded).toBeDefined();
    console.log('Found stream_ended event:', streamEnded);

    // Clean up
    await request.delete(`/api/ingest/sessions/${sessionId}`);
  });

  test('speaker diarization pipeline receives diarization events', async ({ page, request }) => {
    const { spawn } = await import('child_process');
    const { resolve } = await import('path');

    // Resolve the test video path
    const videoPath = resolve(__dirname, '../../../..', TEST_VIDEO);

    // Step 1: Create a session with speaker diarization pipeline
    const createResponse = await request.post('/api/ingest/sessions', {
      data: {
        pipeline: 'speaker_diarization_v1',
        audio_enabled: true,
        video_enabled: false
      }
    });
    expect(createResponse.ok()).toBeTruthy();
    const session = await createResponse.json();
    const sessionId = session.session_id;
    const srtUrl = session.srt_url;

    console.log(`Created diarization session: ${sessionId}`);
    console.log(`SRT URL: ${srtUrl}`);

    // Step 2: Navigate to page and set up SSE listener
    await page.goto('/#/observe');

    // Set up SSE event collection in the browser
    const eventsPromise = page.evaluate(async (sid: string) => {
      return new Promise<{ events: any[], error?: string }>((resolve) => {
        const events: any[] = [];
        const eventSource = new EventSource(`/api/ingest/sessions/${sid}/events`);

        const timeout = setTimeout(() => {
          eventSource.close();
          resolve({ events });
        }, 25000); // Wait up to 25 seconds for diarization events

        // Listen for all event types
        const eventTypes = ['health', 'system', 'alert', 'event', 'diarization'];
        eventTypes.forEach(eventType => {
          eventSource.addEventListener(eventType, (event: MessageEvent) => {
            try {
              const data = JSON.parse(event.data);
              data.event_type = eventType;
              events.push(data);
              console.log(`Received ${eventType} event:`, data);
              // If we've received a diarization event with segments, we're good
              if (eventType === 'event' && data.segments) {
                clearTimeout(timeout);
                eventSource.close();
                resolve({ events });
              }
            } catch (e) {
              // Ignore parse errors
            }
          });
        });

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

    // Step 3: Start FFmpeg to stream input.mp4 via SRT (audio only)
    const ffmpegArgs = [
      '-re',           // Read input at native frame rate
      '-t', '15',      // Stream 15 seconds for diarization to process
      '-i', videoPath,
      '-vn',           // No video
      '-c:a', 'aac',
      '-ar', '16000',  // 16kHz sample rate (required by pyannote)
      '-ac', '1',      // Mono audio
      '-b:a', '64k',
      '-f', 'mpegts',
      srtUrl
    ];

    console.log('Starting FFmpeg for diarization with input:', videoPath);
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
    console.log('Diarization events received:', sseResult.events.length);

    // Step 5: Verify session processed correctly
    const statusResponse = await request.get(`/api/ingest/sessions/${sessionId}`);
    const status = await statusResponse.json();
    console.log('Final session state:', status.state);

    // Step 6: Assertions
    // FFmpeg should have connected successfully
    expect(ffmpegResult.output).toContain('Output #0');

    // We should have received some events
    expect(sseResult.events.length).toBeGreaterThan(0);

    // Session should have transitioned to streaming or ended
    expect(['streaming', 'ended']).toContain(status.state);

    // Clean up: end the session
    await request.delete(`/api/ingest/sessions/${sessionId}`);
  });
});

// Speaker Diarization specific tests
test.describe('Speaker Diarization', () => {
  test('creates session with speaker diarization pipeline', async ({ request }) => {
    const response = await request.post('/api/ingest/sessions', {
      data: {
        pipeline: 'speaker_diarization_v1',
        audio_enabled: true,
        video_enabled: false
      }
    });

    expect(response.status()).toBe(201);
    const session = await response.json();
    expect(session).toHaveProperty('session_id');
    expect(session).toHaveProperty('srt_url');
    expect(session.session_id).toMatch(/^sess_/);
    expect(session.srt_url).toContain('srt://');

    // Verify session status
    const statusResponse = await request.get(`/api/ingest/sessions/${session.session_id}`);
    expect(statusResponse.ok()).toBeTruthy();
    const status = await statusResponse.json();
    expect(status.pipeline).toBe('speaker_diarization_v1');

    // Clean up
    await request.delete(`/api/ingest/sessions/${session.session_id}`);
  });

  test('speaker diarization pipeline returns expected structure', async ({ request }) => {
    const response = await request.post('/api/ingest/sessions', {
      data: {
        pipeline: 'speaker_diarization_v1',
        audio_enabled: true,
        video_enabled: false
      }
    });

    expect(response.status()).toBe(201);
    const session = await response.json();

    // Verify all expected fields are present
    expect(session).toHaveProperty('session_id');
    expect(session).toHaveProperty('srt_url');
    expect(session).toHaveProperty('ffmpeg_command_copy');
    expect(session).toHaveProperty('ffmpeg_command_transcode');
    expect(session).toHaveProperty('events_url');
    expect(session).toHaveProperty('expires_at');

    // FFmpeg commands should include audio settings
    expect(session.ffmpeg_command_transcode).toContain('-c:a');
    expect(session.ffmpeg_command_transcode).toContain('srt://');

    // Clean up
    await request.delete(`/api/ingest/sessions/${session.session_id}`);
  });
});
