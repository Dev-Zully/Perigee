'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const WEB_ROOT = path.join(__dirname, '..');
const REPO_ROOT = path.join(WEB_ROOT, '..');
const WALLET_ICON_HOST = 'stellar.creit.tech';

function readRelative(root, filePath) {
  return fs.readFileSync(path.join(root, filePath), 'utf8');
}

test('wallet modal renders wallet icons through next/image with fixed dimensions', () => {
  const source = readRelative(WEB_ROOT, path.join('components', 'WalletModal.tsx'));

  assert.match(source, /from\s+["']next\/image["']/);
  assert.match(source, /<Image[\s\S]*src=\{wallet\.icon\}/);
  assert.match(source, /<Image[\s\S]*width=\{?20\}?/);
  assert.match(source, /<Image[\s\S]*height=\{?20\}?/);
});

test('Next image config allows the remote wallet icon host', () => {
  const configPaths = [
    path.join('web', 'next.config.js'),
    path.join('Perigee', 'web', 'next.config.js'),
  ];

  for (const configPath of configPaths) {
    const source = readRelative(REPO_ROOT, configPath);

    assert.ok(source.includes('images'), `${configPath} must configure images`);
    assert.ok(source.includes('remotePatterns'), `${configPath} must define remotePatterns`);
    assert.ok(source.includes(WALLET_ICON_HOST), `${configPath} must allow ${WALLET_ICON_HOST}`);
  }
});
