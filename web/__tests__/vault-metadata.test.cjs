// vault-metadata.test.cjs — unit tests for vault SEO dynamic metadata and JSON-LD structured data (Issue #229 / FE-022)
//
// Runs with: node --test ./__tests__/vault-metadata.test.cjs

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

// ── Pure logic mirroring web/lib/vault-metadata.ts ──────────────────────────

const SITE_ORIGIN =
  process.env.NEXT_PUBLIC_SITE_URL?.replace(/\/+$/, "") || "https://perigee.app";
const DEFAULT_OG_IMAGE = "/og-default.png";

function normalizeVaultData(raw, fallbackId) {
  if (!raw) {
    return {
      id: fallbackId || "unknown",
      name: "Vault Not Found",
      balance: 0,
      asset: "XLM",
      apy: 0,
      owner: "Unknown",
      status: "PENDING",
    };
  }

  let parsedConfig = {};
  if ("config_json" in raw && typeof raw.config_json === "string" && raw.config_json) {
    try {
      parsedConfig = JSON.parse(raw.config_json);
    } catch {
      // ignore
    }
  }

  const id = String(raw.id || fallbackId || "unknown");
  const name = String(raw.name || parsedConfig.name || `Vault ${id}`);
  const balance = Number(raw.balance ?? parsedConfig.balance ?? 0);
  const asset = String(raw.asset || parsedConfig.asset || "XLM");
  const apy = Number(raw.apy ?? parsedConfig.apy ?? 0);
  const owner = String(raw.owner || ("manager_id" in raw ? raw.manager_id : "") || parsedConfig.owner || "Perigee");
  const status = String(raw.status || "ACTIVE");

  return { id, name, balance, asset, apy, owner, status };
}

function buildVaultOgTags(input, fallbackId) {
  const vault = normalizeVaultData(input, fallbackId);
  return {
    "vault:name": vault.name,
    "vault:balance": `${vault.balance} ${vault.asset}`,
    "vault:apy": `${vault.apy}%`,
    "vault:asset": vault.asset,
    "vault:status": vault.status,
    "og:balance": `${vault.balance} ${vault.asset}`,
    "og:apy": `${vault.apy}%`,
  };
}

function buildVaultJsonLd(input, fallbackId, siteUrl = SITE_ORIGIN) {
  const vault = normalizeVaultData(input, fallbackId);
  const canonicalUrl = `${siteUrl}/vault/${vault.id}`;

  return {
    "@context": "https://schema.org",
    "@type": "FinancialProduct",
    name: vault.name,
    description: `Perigee autonomous vault ${vault.name} on Stellar. Current Balance: ${vault.balance} ${vault.asset}, APY: ${vault.apy}%.`,
    url: canonicalUrl,
    provider: {
      "@type": "Organization",
      name: "Perigee",
      url: siteUrl,
    },
    offers: {
      "@type": "Offer",
      price: String(vault.balance),
      priceCurrency: vault.asset,
    },
    annualPercentageYield: `${vault.apy}%`,
  };
}

function buildVaultMetadata(input, fallbackId, siteUrl = SITE_ORIGIN) {
  const vault = normalizeVaultData(input, fallbackId);
  const title = `${vault.name} | Perigee`;
  const description = `Perigee vault ${vault.name}. Balance: ${vault.balance} ${vault.asset} · APY: ${vault.apy}% · Status: ${vault.status}.`;
  const canonicalUrl = `${siteUrl}/vault/${vault.id}`;
  const ogDescription = `Vault: ${vault.name} | Balance: ${vault.balance} ${vault.asset} | APY: ${vault.apy}%`;

  const ogTags = buildVaultOgTags(input, fallbackId);

  return {
    title,
    description,
    alternates: {
      canonical: canonicalUrl,
    },
    openGraph: {
      title,
      description: ogDescription,
      url: canonicalUrl,
      siteName: "Perigee",
      type: "website",
      images: [
        {
          url: DEFAULT_OG_IMAGE,
          width: 1200,
          height: 630,
          alt: `${vault.name} - Perigee Vault`,
        },
      ],
    },
    twitter: {
      card: "summary_large_image",
      title,
      description: ogDescription,
      images: [DEFAULT_OG_IMAGE],
    },
    other: ogTags,
  };
}

// ── Tests ───────────────────────────────────────────────────────────────────

test('buildVaultMetadata: generates complete Open Graph metadata with name, balance, and APY', () => {
  const vault = {
    id: "vault-101",
    name: "Alpha Yield Optimizer",
    balance: 50000,
    asset: "USDC",
    apy: 14.5,
    owner: "GA7QYNF7SOWQ3GLR2BGMZEHXAVIRZA4KVWLTJJFC7MGXUA74P7UJVTHG",
    status: "ACTIVE",
  };

  const metadata = buildVaultMetadata(vault, "vault-101");

  // Title & description checks
  assert.equal(metadata.title, "Alpha Yield Optimizer | Perigee");
  assert.match(metadata.description, /Alpha Yield Optimizer/);
  assert.match(metadata.description, /50000 USDC/);
  assert.match(metadata.description, /14.5%/);

  // Canonical check
  assert.equal(metadata.alternates.canonical, `${SITE_ORIGIN}/vault/vault-101`);

  // Open Graph checks
  assert.equal(metadata.openGraph.title, "Alpha Yield Optimizer | Perigee");
  assert.equal(
    metadata.openGraph.description,
    "Vault: Alpha Yield Optimizer | Balance: 50000 USDC | APY: 14.5%"
  );
  assert.equal(metadata.openGraph.siteName, "Perigee");
  assert.equal(metadata.openGraph.type, "website");
  assert.equal(metadata.openGraph.images[0].url, DEFAULT_OG_IMAGE);

  // Twitter Card checks
  assert.equal(metadata.twitter.card, "summary_large_image");
  assert.equal(metadata.twitter.title, "Alpha Yield Optimizer | Perigee");
  assert.equal(
    metadata.twitter.description,
    "Vault: Alpha Yield Optimizer | Balance: 50000 USDC | APY: 14.5%"
  );

  // Custom OG / Vault tags
  assert.equal(metadata.other["vault:name"], "Alpha Yield Optimizer");
  assert.equal(metadata.other["vault:balance"], "50000 USDC");
  assert.equal(metadata.other["vault:apy"], "14.5%");
  assert.equal(metadata.other["og:balance"], "50000 USDC");
  assert.equal(metadata.other["og:apy"], "14.5%");
});

test('buildVaultMetadata: handles vault records with config_json and missing fields gracefully', () => {
  const vaultRecord = {
    id: "v-456",
    manager_id: "mgr-99",
    name: "Delta Staking Vault",
    status: "ACTIVE",
    config_json: JSON.stringify({ balance: 25000, asset: "XLM", apy: 8.2 }),
    version: 1,
    idempotency_key: null,
    created_at: "2026-08-01T00:00:00Z",
    updated_at: "2026-08-01T00:00:00Z",
  };

  const metadata = buildVaultMetadata(vaultRecord, "v-456");

  assert.equal(metadata.title, "Delta Staking Vault | Perigee");
  assert.equal(
    metadata.openGraph.description,
    "Vault: Delta Staking Vault | Balance: 25000 XLM | APY: 8.2%"
  );
  assert.equal(metadata.other["vault:balance"], "25000 XLM");
  assert.equal(metadata.other["vault:apy"], "8.2%");
});

test('buildVaultMetadata: handles null vault with fallback placeholder metadata', () => {
  const metadata = buildVaultMetadata(null, "missing-id");

  assert.equal(metadata.title, "Vault Not Found | Perigee");
  assert.equal(
    metadata.openGraph.description,
    "Vault: Vault Not Found | Balance: 0 XLM | APY: 0%"
  );
  assert.equal(metadata.other["vault:balance"], "0 XLM");
});

test('buildVaultJsonLd: creates valid Schema.org FinancialProduct structured data', () => {
  const vault = {
    id: "v-789",
    name: "Soroban Liquidity Core",
    balance: 120000,
    asset: "USDC",
    apy: 18.0,
    owner: "GA...",
    status: "ACTIVE",
  };

  const jsonLd = buildVaultJsonLd(vault, "v-789");

  assert.equal(jsonLd["@context"], "https://schema.org");
  assert.equal(jsonLd["@type"], "FinancialProduct");
  assert.equal(jsonLd.name, "Soroban Liquidity Core");
  assert.equal(jsonLd.url, `${SITE_ORIGIN}/vault/v-789`);
  assert.deepEqual(jsonLd.provider, {
    "@type": "Organization",
    name: "Perigee",
    url: SITE_ORIGIN,
  });
  assert.deepEqual(jsonLd.offers, {
    "@type": "Offer",
    price: "120000",
    priceCurrency: "USDC",
  });
  assert.equal(jsonLd.annualPercentageYield, "18%");
});

test('App Router vault page exports generateMetadata and default component', () => {
  const pagePath = path.join(__dirname, '..', 'app', 'vault', '[id]', 'page.tsx');
  assert.ok(fs.existsSync(pagePath), "app/vault/[id]/page.tsx must exist");

  const pageContent = fs.readFileSync(pagePath, 'utf8');
  assert.ok(
    pageContent.includes("export async function generateMetadata"),
    "app/vault/[id]/page.tsx must export generateMetadata"
  );
  assert.ok(
    pageContent.includes("buildVaultMetadata"),
    "app/vault/[id]/page.tsx must use buildVaultMetadata"
  );
  assert.ok(
    pageContent.includes("buildVaultJsonLd"),
    "app/vault/[id]/page.tsx must use buildVaultJsonLd"
  );
  assert.ok(
    pageContent.includes("application/ld+json"),
    "app/vault/[id]/page.tsx must render JSON-LD script"
  );
});

test('SEO component supports jsonLd structured data and custom OG properties', () => {
  const seoPath = path.join(__dirname, '..', 'components', 'SEO.tsx');
  const seoContent = fs.readFileSync(seoPath, 'utf8');

  assert.ok(
    seoContent.includes("jsonLd?:"),
    "SEO component must include jsonLd in SeoProps"
  );
  assert.ok(
    seoContent.includes("customOg?:"),
    "SEO component must include customOg in SeoProps"
  );
  assert.ok(
    seoContent.includes("application/ld+json"),
    "SEO component must render application/ld+json script when jsonLd is passed"
  );
});
