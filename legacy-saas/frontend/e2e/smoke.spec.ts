import { test, expect } from '@playwright/test';

const BASE = 'https://text-to-print-production.up.railway.app';
const EMAIL = 'sakamoro@extoria.co.jp';
const PASSWORD = '@E99l0103';

// ---------------------------------------------------------------------------
// Helper: login and return authenticated page
// ---------------------------------------------------------------------------
async function login(page: import('@playwright/test').Page) {
  await page.goto(`${BASE}/auth/login`);
  await page.fill('input[type="email"]', EMAIL);
  await page.fill('input[type="password"]', PASSWORD);
  await page.click('button[type="submit"]');
  // Wait for redirect to dashboard
  await page.waitForURL('**/dashboard/**', { timeout: 15000 });
}

// ---------------------------------------------------------------------------
// Public pages
// ---------------------------------------------------------------------------

test.describe('Public endpoints', () => {
  test('GET /health returns ok', async ({ request }) => {
    const resp = await request.get(`${BASE}/health`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.status).toBe('ok');
    expect(body.version).toBeDefined();
    expect(body.printers).toBeDefined();
    expect(body.printers.length).toBeGreaterThan(0);
  });

  test('GET /license returns MIT', async ({ request }) => {
    const resp = await request.get(`${BASE}/license`);
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.license).toBe('MIT');
  });

  test('/ redirects to /auth/login', async ({ page }) => {
    await page.goto(BASE);
    await page.waitForURL('**/auth/login');
    expect(page.url()).toContain('/auth/login');
  });
});

// ---------------------------------------------------------------------------
// Auth pages
// ---------------------------------------------------------------------------

test.describe('Auth pages', () => {
  test('login page renders', async ({ page }) => {
    await page.goto(`${BASE}/auth/login`);
    await expect(page.locator('h1')).toContainText('Sign in');
    await expect(page.locator('input[type="email"]')).toBeVisible();
    await expect(page.locator('input[type="password"]')).toBeVisible();
    await expect(page.locator('button[type="submit"]')).toBeVisible();
    await expect(page.locator('a[href="/auth/register"]')).toBeVisible();
  });

  test('register page renders', async ({ page }) => {
    await page.goto(`${BASE}/auth/register`);
    await expect(page.locator('h1')).toContainText('Create an Account');
    await expect(page.locator('input[type="email"]')).toBeVisible();
    await expect(page.locator('input[type="password"]')).toBeVisible();
    await expect(page.locator('button[type="submit"]')).toBeVisible();
    await expect(page.locator('a[href="/auth/login"]')).toBeVisible();
  });

  test('login with valid credentials redirects to dashboard', async ({ page }) => {
    await login(page);
    expect(page.url()).toContain('/dashboard');
  });

  test('login with invalid credentials shows error', async ({ page }) => {
    await page.goto(`${BASE}/auth/login`);
    await page.fill('input[type="email"]', 'invalid@example.com');
    await page.fill('input[type="password"]', 'wrongpassword');
    await page.click('button[type="submit"]');
    // Should show error message
    await expect(page.locator('.bg-destructive\\/10, [class*="destructive"]')).toBeVisible({ timeout: 10000 });
  });
});

// ---------------------------------------------------------------------------
// Dashboard (authenticated)
// ---------------------------------------------------------------------------

test.describe('Dashboard - authenticated', () => {
  test.beforeEach(async ({ page }) => {
    await login(page);
  });

  test('sidebar navigation visible', async ({ page }) => {
    await expect(page.locator('aside')).toBeVisible();
    await expect(page.locator('aside >> text=Generate')).toBeVisible();
    await expect(page.locator('aside >> text=Projects')).toBeVisible();
    await expect(page.locator('aside >> text=History')).toBeVisible();
    await expect(page.locator('aside >> text=Billing')).toBeVisible();
    await expect(page.locator('aside >> text=Settings')).toBeVisible();
    await expect(page.locator('aside >> text=Sign out')).toBeVisible();
  });

  test('projects page renders', async ({ page }) => {
    await page.click('text=Projects');
    await page.waitForURL('**/dashboard/projects');
    await expect(page.locator('h1')).toContainText('Projects');
    await expect(page.locator('input[placeholder="New project name"]')).toBeVisible();
  });

  test('create and delete project', async ({ page }) => {
    await page.click('text=Projects');
    await page.waitForURL('**/dashboard/projects');

    const projectName = `test-${Date.now()}`;
    await page.fill('input[placeholder="New project name"]', projectName);
    await page.click('button:has-text("Create")');

    // Should redirect to console
    await page.waitForURL('**/dashboard/console**', { timeout: 10000 });
    await expect(page.locator(`text=${projectName}`)).toBeVisible();

    // Go back to projects
    await page.click('text=Projects');
    await page.waitForURL('**/dashboard/projects');

    // Delete the project
    const projectCard = page.locator(`button:has-text("${projectName}")`);
    await projectCard.hover();
    page.on('dialog', (dialog) => dialog.accept());
    await projectCard.locator('text=Delete').click();

    // Project should be gone
    await expect(projectCard).not.toBeVisible({ timeout: 5000 });
  });

  test('generate page renders with mode toggle', async ({ page }) => {
    await page.click('text=Generate');
    await page.waitForURL('**/dashboard');
    await expect(page.locator('h1')).toContainText('text-to-print');
    await expect(page.locator('button:has-text("Natural Language")')).toBeVisible();
    await expect(page.locator('button:has-text("LOL DSL")')).toBeVisible();
    await expect(page.locator('button:has-text("Generate .3mf")')).toBeVisible();

    // Quality buttons
    await expect(page.locator('button:has-text("preview")')).toBeVisible();
    await expect(page.locator('button:has-text("high")')).toBeVisible();
    await expect(page.locator('button:has-text("ultra")')).toBeVisible();
  });

  test('generate page LOL DSL mode toggle', async ({ page }) => {
    await page.click('text=Generate');
    await page.waitForURL('**/dashboard');

    // Switch to LOL DSL mode
    await page.click('button:has-text("LOL DSL")');
    await expect(page.locator('textarea[placeholder*="smooth_union"]')).toBeVisible();

    // Switch back
    await page.click('button:has-text("Natural Language")');
    await expect(page.locator('textarea[placeholder*="phone stand"]')).toBeVisible();
  });

  test('history page renders', async ({ page }) => {
    await page.click('text=History');
    await page.waitForURL('**/dashboard/history');
    await expect(page.locator('h1')).toContainText('Generation History');
  });

  test('settings page shows email and plan', async ({ page }) => {
    await page.click('text=Settings');
    await page.waitForURL('**/dashboard/settings');
    await expect(page.locator('h1')).toContainText('Settings');
    // Email should be displayed
    await expect(page.locator(`text=${EMAIL}`)).toBeVisible({ timeout: 10000 });
    // Plan should show Enterprise
    await expect(page.locator('text=Enterprise')).toBeVisible({ timeout: 10000 });
    // API Key generate button
    await expect(page.locator('button:has-text("Generate")')).toBeVisible();
  });

  test('sign out returns to login', async ({ page }) => {
    await page.click('text=Sign out');
    await page.waitForURL('**/auth/login', { timeout: 10000 });
    expect(page.url()).toContain('/auth/login');
  });
});

// ---------------------------------------------------------------------------
// API with key
// ---------------------------------------------------------------------------

test.describe('API with key', () => {
  test('unauthenticated /api/v1 returns 401', async ({ request }) => {
    const resp = await request.post(`${BASE}/api/v1/generate`, {
      headers: { 'Content-Type': 'application/json' },
      data: { prompt: 'test' },
    });
    expect(resp.status()).toBe(401);
  });

  test('generate-lol with API key returns LOL source', async ({ request }) => {
    const resp = await request.post(`${BASE}/api/v1/generate-lol`, {
      headers: {
        'X-API-Key': 'ak_3c5d48d37b414b829faa16337e261d68',
        'Content-Type': 'application/json',
      },
      data: { lol_source: 'sphere { radius: 20 }' },
    });
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.status).toBe('completed');
    expect(body.lol_source).toBe('sphere { radius: 20 }');
    expect(body.job_id).toBeDefined();
  });
});
