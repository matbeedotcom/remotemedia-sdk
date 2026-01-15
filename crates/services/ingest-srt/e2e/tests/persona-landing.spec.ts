import { test, expect } from '@playwright/test';

test.describe('Persona Landing Pages', () => {
  const personas = [
    { slug: 'voice-agents', name: 'AI Voice Agents', headline: 'voice agent hesitates' },
    { slug: 'contact-center', name: 'Contact Center QA', headline: 'dead air, talk-over' },
    { slug: 'telehealth', name: 'Telehealth', headline: 'patients can\'t hear you' },
    { slug: 'broadcast', name: 'Broadcast / Streaming', headline: 'silence, clipping, and frozen video' },
  ];

  test.describe('Landing Page Content', () => {
    for (const persona of personas) {
      test(`${persona.name} landing page renders correctly`, async ({ page }) => {
        await page.goto(`/#/${persona.slug}`);

        // Verify hero headline contains persona-specific text
        await expect(page.getByRole('heading', { level: 1 })).toContainText(persona.headline, { ignoreCase: true });

        // Verify CTA button is present
        await expect(page.getByRole('button', { name: /observe a live stream/i })).toBeVisible();

        // Verify trust text is present
        await expect(page.getByText(/no sign-up required/i)).toBeVisible();

        // Verify secondary CTA for demo mode
        await expect(page.getByText(/try a known failure scenario/i)).toBeVisible();
      });
    }

    test('general (root) landing page renders correctly', async ({ page }) => {
      await page.goto('/#/');

      // Verify general persona headline
      await expect(page.getByRole('heading', { level: 1 })).toContainText('health of live media', { ignoreCase: true });

      // Verify CTA button is present
      await expect(page.getByRole('button', { name: /observe a live stream/i })).toBeVisible();
    });

    test('invalid path redirects to root landing', async ({ page }) => {
      await page.goto('/#/invalid-persona-slug');

      // Should redirect to root which shows general persona
      await expect(page.getByRole('heading', { level: 1 })).toContainText('health of live media', { ignoreCase: true });
    });
  });

  test.describe('Landing to Observer Navigation', () => {
    test('CTA navigates to observer with persona param', async ({ page }) => {
      await page.goto('/#/voice-agents');

      // Click the primary CTA
      await page.getByRole('button', { name: /observe a live stream/i }).first().click();

      // Verify URL includes persona param
      await expect(page).toHaveURL(/\/observe\?persona=voice-agents/);

      // Verify observer UI loads (shows pipeline selection)
      await expect(page.getByText(/waiting for media/i)).toBeVisible();
    });

    test('demo CTA navigates with demo=true param', async ({ page }) => {
      await page.goto('/#/voice-agents');

      // Click the secondary CTA for demo mode
      await page.getByText(/try a known failure scenario/i).first().click();

      // Verify URL includes both persona and demo params
      await expect(page).toHaveURL(/\/observe\?persona=voice-agents&demo=true/);
    });
  });

  test.describe('Pipeline Preselection', () => {
    test('voice-agents preselects AI Voice Agent Health pipeline', async ({ page }) => {
      await page.goto('/#/voice-agents');
      await page.getByRole('button', { name: /observe a live stream/i }).first().click();

      // Wait for observer to load
      await expect(page.getByText(/waiting for media/i)).toBeVisible();

      // AI Voice Agent Health should be selected (has border styling)
      await expect(page.getByText('AI Voice Agent Health')).toBeVisible();
    });

    test('contact-center preselects Contact Center QA pipeline', async ({ page }) => {
      await page.goto('/#/contact-center');
      await page.getByRole('button', { name: /observe a live stream/i }).first().click();

      // Wait for observer to load
      await expect(page.getByText(/waiting for media/i)).toBeVisible();

      // Contact Center QA should be selected
      await expect(page.getByText('Contact Center QA')).toBeVisible();
    });
  });

  test.describe('Demo Mode Indicator', () => {
    test('demo mode shows DEMO badge when session active', async ({ page }) => {
      // Navigate to observer in demo mode
      await page.goto('/#/observe?persona=voice-agents&demo=true');

      // Select a pipeline and create session
      await page.getByText('AI Voice Agent Health').click();

      // Create session
      const createButton = page.getByRole('button', { name: /create session/i });
      if (await createButton.isVisible()) {
        await createButton.click();

        // Wait for session to be active
        await expect(page.getByText('READY')).toBeVisible({ timeout: 10000 });

        // Verify DEMO badge is visible
        await expect(page.getByText('DEMO')).toBeVisible();
      }
    });
  });

  test.describe('Persona Context Persistence', () => {
    test('persona context persists across page refresh', async ({ page }) => {
      // Navigate via landing page
      await page.goto('/#/voice-agents');
      await page.getByRole('button', { name: /observe a live stream/i }).first().click();

      // Wait for observer to load
      await expect(page).toHaveURL(/\/observe\?persona=voice-agents/);

      // Refresh the page
      await page.reload();

      // URL should still have persona param
      await expect(page).toHaveURL(/persona=voice-agents/);

      // Pipeline should still be visible
      await expect(page.getByText('AI Voice Agent Health')).toBeVisible();
    });
  });

  test.describe('Landing Page Sections', () => {
    test('landing page has all required sections', async ({ page }) => {
      await page.goto('/#/voice-agents');

      // Hero section
      await expect(page.getByRole('heading', { level: 1 })).toBeVisible();

      // Problem section
      await expect(page.getByText(/why this breaks/i)).toBeVisible();

      // Solution section - generic copy about side-car
      await expect(page.getByText(/side-car/i)).toBeVisible();

      // What demo shows section
      await expect(page.getByText(/what this demo shows/i)).toBeVisible();

      // How it works section
      await expect(page.getByText(/how the demo works/i)).toBeVisible();

      // Trust section
      await expect(page.getByText(/built for production/i)).toBeVisible();
    });
  });

  test.describe('Performance', () => {
    test('landing page loads within 2 seconds', async ({ page }) => {
      const startTime = Date.now();
      await page.goto('/#/voice-agents');

      // Wait for hero to be visible
      await expect(page.getByRole('heading', { level: 1 })).toBeVisible();

      const loadTime = Date.now() - startTime;
      expect(loadTime).toBeLessThan(2000);
    });
  });
});
