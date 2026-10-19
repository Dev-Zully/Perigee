// featureFlagService.test.cjs — unit tests for web/features/feature-flags/
// Covers: defaults, env-var overrides, enable/disable/toggle, API fetch,
// reset, and start/stop polling.
//
// Runs with: node --test ./features/feature-flags/featureFlagService.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

// ── helpers ──────────────────────────────────────────────────────────────────

function freshService() {
  // Each test file gets its own import cycle via require cache clear.
  // We re-import the module to get a pristine singleton.
  delete require.cache[require.resolve('./constants')];
  delete require.cache[require.resolve('./feature-flags')];
  delete require.cache[require.resolve('./types')];
  delete require.cache[require.resolve('./featureFlagService')];
  const mod = require('./featureFlagService');
  return mod.featureFlagService;
}

// ── defaults ─────────────────────────────────────────────────────────────────

test('featureFlagService: all flags have boolean values', () => {
  const svc = freshService();
  const flags = svc.getAll();
  for (const [key, value] of Object.entries(flags)) {
    assert.equal(typeof value, 'boolean', `flag ${key} should be boolean`);
  }
});

test('featureFlagService: default values match feature-flags.ts definitions', () => {
  const svc = freshService();
  assert.equal(svc.isEnabled('newVaultUI'), false);
  assert.equal(svc.isEnabled('dashboardV2'), false);
  assert.equal(svc.isEnabled('experimentalCharts'), true);
  assert.equal(svc.isEnabled('notificationsV2'), false);
});

// ── enable / disable / toggle / set ─────────────────────────────────────────

test('featureFlagService: enable sets flag to true', () => {
  const svc = freshService();
  svc.enable('newVaultUI');
  assert.equal(svc.isEnabled('newVaultUI'), true);
});

test('featureFlagService: disable sets flag to false', () => {
  const svc = freshService();
  svc.disable('experimentalCharts');
  assert.equal(svc.isEnabled('experimentalCharts'), false);
});

test('featureFlagService: toggle flips the flag', () => {
  const svc = freshService();
  assert.equal(svc.isEnabled('dashboardV2'), false);
  svc.toggle('dashboardV2');
  assert.equal(svc.isEnabled('dashboardV2'), true);
  svc.toggle('dashboardV2');
  assert.equal(svc.isEnabled('dashboardV2'), false);
});

test('featureFlagService: set allows arbitrary boolean value', () => {
  const svc = freshService();
  svc.set('notificationsV2', true);
  assert.equal(svc.isEnabled('notificationsV2'), true);
  svc.set('notificationsV2', false);
  assert.equal(svc.isEnabled('notificationsV2'), false);
});

// ── reset ───────────────────────────────────────────────────────────────────

test('featureFlagService: reset restores defaults', () => {
  const svc = freshService();
  svc.enable('newVaultUI');
  svc.disable('experimentalCharts');
  svc.reset();
  assert.equal(svc.isEnabled('newVaultUI'), false);
  assert.equal(svc.isEnabled('experimentalCharts'), true);
});

// ── env-var overrides ───────────────────────────────────────────────────────

test('featureFlagService: applyEnvOverrides reads NEXT_PUBLIC_FEATURE_FLAG_ prefix', () => {
  const svc = freshService();
  const key = 'NEXT_PUBLIC_FEATURE_FLAG_newVaultUI';
  const prev = process.env[key];
  try {
    process.env[key] = 'true';
    svc.applyEnvOverrides();
    assert.equal(svc.isEnabled('newVaultUI'), true);
  } finally {
    if (prev === undefined) delete process.env[key];
    else process.env[key] = prev;
  }
});

test('featureFlagService: applyEnvOverrides treats non-"true" as false', () => {
  const svc = freshService();
  const key = 'NEXT_PUBLIC_FEATURE_FLAG_experimentalCharts';
  const prev = process.env[key];
  try {
    process.env[key] = 'nope';
    svc.applyEnvOverrides();
    assert.equal(svc.isEnabled('experimentalCharts'), false);
  } finally {
    if (prev === undefined) delete process.env[key];
    else process.env[key] = prev;
  }
});

test('featureFlagService: applyEnvOverrides supports custom prefix', () => {
  const svc = freshService();
  const key = 'CUSTOM_PREFIX_dashboardV2';
  const prev = process.env[key];
  try {
    process.env[key] = 'true';
    svc.applyEnvOverrides('CUSTOM_PREFIX_');
    assert.equal(svc.isEnabled('dashboardV2'), true);
  } finally {
    if (prev === undefined) delete process.env[key];
    else process.env[key] = prev;
  }
});

// ── API fetch ───────────────────────────────────────────────────────────────

test('featureFlagService: fetchFromApi updates flags from JSON response', async () => {
  const svc = freshService();
  const originalFetch = globalThis.fetch;

  globalThis.fetch = async () =>
    new Response(
      JSON.stringify({ newVaultUI: true, dashboardV2: true }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    );

  try {
    await svc.fetchFromApi({ url: 'https://example.com/flags' });
    assert.equal(svc.isEnabled('newVaultUI'), true);
    assert.equal(svc.isEnabled('dashboardV2'), true);
    // experimentalCharts unchanged (not in response)
    assert.equal(svc.isEnabled('experimentalCharts'), true);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test('featureFlagService: fetchFromApi ignores non-boolean values', async () => {
  const svc = freshService();
  const originalFetch = globalThis.fetch;

  globalThis.fetch = async () =>
    new Response(
      JSON.stringify({ newVaultUI: 'yes', dashboardV2: 42 }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    );

  try {
    await svc.fetchFromApi({ url: 'https://example.com/flags' });
    assert.equal(svc.isEnabled('newVaultUI'), false);
    assert.equal(svc.isEnabled('dashboardV2'), false);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test('featureFlagService: fetchFromApi ignores unknown flag keys', async () => {
  const svc = freshService();
  const originalFetch = globalThis.fetch;

  globalThis.fetch = async () =>
    new Response(
      JSON.stringify({ unknownFlag: true }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    );

  try {
    await svc.fetchFromApi({ url: 'https://example.com/flags' });
    // No flags should have changed
    assert.equal(svc.isEnabled('newVaultUI'), false);
    assert.equal(svc.isEnabled('experimentalCharts'), true);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test('featureFlagService: fetchFromApi handles network errors gracefully', async () => {
  const svc = freshService();
  svc.enable('newVaultUI');
  const originalFetch = globalThis.fetch;

  globalThis.fetch = async () => {
    throw new Error('Network failure');
  };

  try {
    await svc.fetchFromApi({ url: 'https://example.com/flags' });
    // Flag should retain its previous state
    assert.equal(svc.isEnabled('newVaultUI'), true);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test('featureFlagService: fetchFromApi handles non-ok responses', async () => {
  const svc = freshService();
  const originalFetch = globalThis.fetch;

  globalThis.fetch = async () =>
    new Response('Not Found', { status: 404 });

  try {
    await svc.fetchFromApi({ url: 'https://example.com/flags' });
    // Flags unchanged
    assert.equal(svc.isEnabled('newVaultUI'), false);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

// ── initialize ──────────────────────────────────────────────────────────────

test('featureFlagService: initialize resets, applies env, and fetches API', async () => {
  const svc = freshService();
  const key = 'NEXT_PUBLIC_FEATURE_FLAG_newVaultUI';
  const prev = process.env[key];
  const originalFetch = globalThis.fetch;

  globalThis.fetch = async () =>
    new Response(
      JSON.stringify({ dashboardV2: true }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    );

  try {
    svc.enable('notificationsV2');
    process.env[key] = 'true';
    await svc.initialize({ apiSource: { url: 'https://example.com/flags' } });
    assert.equal(svc.isEnabled('newVaultUI'), true);
    assert.equal(svc.isEnabled('dashboardV2'), true);
    // notificationsV2 was enabled before initialize, but reset clears it
    assert.equal(svc.isEnabled('notificationsV2'), false);
  } finally {
    if (prev === undefined) delete process.env[key];
    else process.env[key] = prev;
    globalThis.fetch = originalFetch;
    svc.stopPolling();
  }
});

// ── stopPolling ─────────────────────────────────────────────────────────────

test('featureFlagService: stopPolling clears timer without error', () => {
  const svc = freshService();
  svc.stopPolling();
  svc.stopPolling(); // double-stop should be safe
});
