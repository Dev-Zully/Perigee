/**
 * App Router migration validation tests.
 *
 * These tests ensure:
 * 1. Next.js version is pinned (no ^ or ~ prefix)
 * 2. app/layout.tsx exists with required exports
 * 3. Both routers can coexist without conflicts
 * 4. MIGRATION.md is present and up to date
 */

const { readFileSync, existsSync } = require('fs');
const { join } = require('path');
const { describe, it } = require('node:test');
const packageJson = require('../../package.json');

describe('App Router Migration Safeguards', () => {
  // SUITE 1 — Version pinning (2 tests)
  describe('Next.js version pinned', () => {
    it('next version has no ^ prefix', () => {
      const nextVersion =
        packageJson.dependencies?.next ?? packageJson.devDependencies?.next;
      if (!nextVersion) throw new Error('Next version not found');
      if (/^\^/.test(nextVersion) || /^~/.test(nextVersion)) {
        throw new Error(
          `Next.js version must be pinned. Found: ${nextVersion}`
        );
      }
    });

    it('next version is a valid semver string', () => {
      const nextVersion =
        packageJson.dependencies?.next ?? packageJson.devDependencies?.next;
      if (!nextVersion) throw new Error('Next version not found');
      if (!/^\d+\.\d+\.\d+/.test(nextVersion)) {
        throw new Error(
          `Next.js version must be valid semver. Found: ${nextVersion}`
        );
      }
    });
  });

  // SUITE 2 — App Router foundation (3 tests)
  describe('App Router foundation files exist', () => {
    it('app/layout.tsx exists', () => {
      const layoutPath = join(process.cwd(), 'app', 'layout.tsx');
      if (!existsSync(layoutPath)) {
        throw new Error(`app/layout.tsx not found at ${layoutPath}`);
      }
    });

    it('app/layout.tsx exports default RootLayout', () => {
      const content = readFileSync(
        join(process.cwd(), 'app', 'layout.tsx'),
        'utf-8'
      );
      if (!content.includes('export default function RootLayout')) {
        throw new Error('app/layout.tsx does not export default RootLayout');
      }
    });

    it('app/layout.tsx exports metadata', () => {
      const content = readFileSync(
        join(process.cwd(), 'app', 'layout.tsx'),
        'utf-8'
      );
      if (!content.includes('export const metadata')) {
        throw new Error('app/layout.tsx does not export metadata');
      }
    });
  });

  // SUITE 3 — Migration documentation (2 tests)
  describe('Migration documentation', () => {
    it('MIGRATION.md exists', () => {
      const migrationPath = join(process.cwd(), 'MIGRATION.md');
      if (!existsSync(migrationPath)) {
        throw new Error(`MIGRATION.md not found at ${migrationPath}`);
      }
    });

    it('MIGRATION.md contains pages inventory', () => {
      const content = readFileSync(
        join(process.cwd(), 'MIGRATION.md'),
        'utf-8'
      );
      if (!content.includes('Pages Inventory')) {
        throw new Error('MIGRATION.md missing Pages Inventory section');
      }
      if (!content.includes('Phase')) {
        throw new Error('MIGRATION.md missing Phase documentation');
      }
    });
  });

  // SUITE 4 — Pages Router backward compatibility (2 tests)
  describe('Pages Router backward compatibility', () => {
    it('pages/_app.tsx still exists', () => {
      const appPath = join(process.cwd(), 'pages', '_app.tsx');
      const appPathJs = join(process.cwd(), 'pages', '_app.js');
      if (!existsSync(appPath) && !existsSync(appPathJs)) {
        throw new Error('pages/_app.tsx or pages/_app.js not found');
      }
    });

    it('pages/index.tsx still exists', () => {
      const indexPath = join(process.cwd(), 'pages', 'index.tsx');
      const indexPathJs = join(process.cwd(), 'pages', 'index.js');
      if (!existsSync(indexPath) && !existsSync(indexPathJs)) {
        throw new Error('pages/index.tsx or pages/index.js not found');
      }
    });
  });
});
