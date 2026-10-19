// vault-routing.test.cjs — unit tests for WEB-55 / #188
// The vault detail page must derive its ID from URL path params with stable
// state hydration, never from `router.query` (which can desync on navigation).
//
// Runs with: node --test ./__tests__/vault-routing.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const WEB_ROOT = path.join(__dirname, '..');

function readWebFile(relPath) {
  return fs.readFileSync(path.join(WEB_ROOT, relPath), 'utf8');
}

test('Pages Router vault page must not rely on router.query', () => {
  const pagePath = path.join('pages', 'vault', '[id].tsx');
  assert.ok(
    fs.existsSync(path.join(WEB_ROOT, pagePath)),
    'pages/vault/[id].tsx must exist',
  );

  const source = readWebFile(pagePath);

  assert.doesNotMatch(
    source,
    /=\s*router\.query/,
    'pages/vault/[id].tsx must not read the ID from router.query',
  );
  assert.doesNotMatch(
    source,
    /useRouter/,
    'pages/vault/[id].tsx must not import useRouter',
  );
});

test('Pages Router vault page resolves the ID from path params via getServerSideProps', () => {
  const source = readWebFile(path.join('pages', 'vault', '[id].tsx'));

  assert.match(
    source,
    /getServerSideProps/,
    'pages/vault/[id].tsx must export getServerSideProps',
  );
  assert.match(
    source,
    /context\.params\?\.id/,
    'getServerSideProps must read the vault ID from context.params',
  );
  assert.match(
    source,
    /props:\s*\{\s*vaultId/,
    'getServerSideProps must hydrate the vaultId through props',
  );
});

test('Pages Router vault page receives server-hydrated vault props', () => {
  const source = readWebFile(path.join('pages', 'vault', '[id].tsx'));

  assert.match(
    source,
    /vaultId:\s*string;\s*vault:\s*Vault\s*\|\s*null/,
    'the page must declare vaultId and vault props for stable hydration',
  );
  assert.match(
    source,
    /function PagesVaultDetailPage\(\{[\s\S]*?vaultId[\s\S]*?vault[\s\S]*?\}: VaultDetailPageProps\)/,
    'the page component must destructure vaultId and vault from props',
  );
});

test('App Router vault page already uses path params', () => {
  const pagePath = path.join('app', 'vault', '[id]', 'page.tsx');
  assert.ok(
    fs.existsSync(path.join(WEB_ROOT, pagePath)),
    'app/vault/[id]/page.tsx must exist',
  );

  const source = readWebFile(pagePath);

  assert.match(
    source,
    /params:\s*Promise<\{ id: string \}>/,
    'app/vault/[id]/page.tsx must type params as a path param',
  );
  assert.match(
    source,
    /resolvedParams\.id/,
    'app/vault/[id]/page.tsx must read the ID from params',
  );
});
