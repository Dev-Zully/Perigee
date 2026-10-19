// font-optimization.test.cjs — unit tests for WEB-54 / #187
// Custom fonts must load through `next/font` (self-hosted, build-time
// preloaded) instead of an external font stylesheet, to eliminate FOIT.
//
// Runs with: node --test ./__tests__/font-optimization.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const WEB_ROOT = path.join(__dirname, '..');

function readWebFile(relPath) {
  return fs.readFileSync(path.join(WEB_ROOT, relPath), 'utf8');
}

test('App Router root layout loads Inter through next/font/google', () => {
  const layout = readWebFile(path.join('app', 'layout.tsx'));

  assert.match(
    layout,
    /from\s+["']next\/font\/google["']/,
    'app/layout.tsx must import from next/font/google',
  );
  assert.match(
    layout,
    /Inter\s*\(\s*\{/,
    'app/layout.tsx must call Inter() from next/font',
  );
});

test('App Router root layout exposes the font as a CSS variable on <html>', () => {
  const layout = readWebFile(path.join('app', 'layout.tsx'));

  assert.match(
    layout,
    /variable:\s*["']--font-inter["']/,
    'app/layout.tsx must expose the font via the --font-inter variable',
  );
  assert.match(
    layout,
    /<html[^>]*className=\{inter\.variable\}/,
    'app/layout.tsx must apply the font variable to the <html> element',
  );
});

test('Pages Router _app.tsx loads Inter through next/font/google and applies it', () => {
  const app = readWebFile(path.join('pages', '_app.tsx'));

  assert.match(
    app,
    /from\s+["']next\/font\/google["']/,
    'pages/_app.tsx must import from next/font/google',
  );
  assert.match(
    app,
    /Inter\s*\(\s*\{/,
    'pages/_app.tsx must call Inter() from next/font',
  );
  assert.match(
    app,
    /className=\{inter\.className\}/,
    'pages/_app.tsx must apply the generated font class to the app wrapper',
  );
});

test('Tailwind font-sans stack resolves the next/font variable', () => {
  const tailwindConfig = readWebFile('tailwind.config.js');

  assert.match(
    tailwindConfig,
    /fontFamily:\s*\{\s*sans:\s*\["var\(--font-inter\)"/,
    'tailwind.config.js must put var(--font-inter) first in fontFamily.sans',
  );
});

test('no external font stylesheet is loaded from Google Fonts', () => {
  const roots = ['app', 'pages', 'components'];
  const offenders = [];

  for (const root of roots) {
    const dir = path.join(WEB_ROOT, root);
    if (!fs.existsSync(dir)) continue;

    const walk = (current) => {
      for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
        const full = path.join(current, entry.name);
        if (entry.isDirectory()) walk(full);
        else if (/\.(tsx?|jsx?|css)$/.test(entry.name)) {
          const src = fs.readFileSync(full, 'utf8');
          if (/fonts\.(googleapis|gstatic)\.com/i.test(src)) {
            offenders.push(path.relative(WEB_ROOT, full));
          }
        }
      }
    };
    walk(dir);
  }

  assert.deepEqual(
    offenders,
    [],
    'fonts must be self-hosted via next/font, not loaded from an external stylesheet',
  );
});
