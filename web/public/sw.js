/* Perigee service worker.
 *
 * Hand-written (no Workbox dependency) implementation of the cache strategies
 * described in issue #688:
 *   - static assets  : CacheFirst
 *   - images         : CacheFirst with expiration (max entries + max age)
 *   - API GET        : StaleWhileRevalidate, entries older than 5 minutes are
 *                      treated as expired and refetched
 *   - API mutations  : failed (offline) POST/PUT/PATCH/DELETE are queued in
 *                      IndexedDB and replayed through Background Sync
 *
 * A new worker waits until the page sends { type: "SKIP_WAITING" } so users
 * can accept the update prompt before the page is reloaded.
 */

const VERSION = "v1";
const STATIC_CACHE = `perigee-static-${VERSION}`;
const IMAGE_CACHE = `perigee-images-${VERSION}`;
const API_CACHE = `perigee-api-${VERSION}`;
const ALL_CACHES = [STATIC_CACHE, IMAGE_CACHE, API_CACHE];

const API_MAX_AGE_MS = 5 * 60 * 1000;
const IMAGE_MAX_AGE_MS = 30 * 24 * 60 * 60 * 1000;
const IMAGE_MAX_ENTRIES = 60;
const CACHED_AT_HEADER = "x-sw-cached-at";

const SYNC_TAG = "perigee-replay-mutations";
const DB_NAME = "perigee-sw";
const STORE = "mutations";

self.addEventListener("install", () => {
  // Intentionally no skipWaiting(): wait for the user to accept the update.
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const names = await caches.keys();
      await Promise.all(
        names
          .filter((name) => name.startsWith("perigee-") && !ALL_CACHES.includes(name))
          .map((name) => caches.delete(name)),
      );
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("message", (event) => {
  if (event.data && event.data.type === "SKIP_WAITING") {
    self.skipWaiting();
  }
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function isStaticAsset(url) {
  return (
    url.pathname.startsWith("/_next/static/") ||
    /\.(?:js|css|woff2?|ttf|otf|wasm)$/.test(url.pathname)
  );
}

function isApi(url) {
  return url.origin === self.location.origin && url.pathname.startsWith("/api/");
}

/** Clone a response and stamp it with the time it was cached. */
async function stamp(response) {
  const headers = new Headers(response.headers);
  headers.set(CACHED_AT_HEADER, String(Date.now()));
  return new Response(await response.clone().blob(), {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

function ageOf(response) {
  const cachedAt = Number(response.headers.get(CACHED_AT_HEADER));
  return Number.isFinite(cachedAt) && cachedAt > 0 ? Date.now() - cachedAt : Infinity;
}

async function cacheFirst(request, cacheName, maxAgeMs) {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(request);
  if (cached && (maxAgeMs === undefined || ageOf(cached) <= maxAgeMs)) {
    return cached;
  }
  try {
    const response = await fetch(request);
    if (response.ok || response.type === "opaque") {
      await cache.put(request, await stamp(response));
      if (cacheName === IMAGE_CACHE) await trimImageCache(cache);
    }
    return response;
  } catch (error) {
    if (cached) return cached;
    throw error;
  }
}

async function trimImageCache(cache) {
  const keys = await cache.keys();
  const overflow = keys.length - IMAGE_MAX_ENTRIES;
  for (let i = 0; i < overflow; i += 1) {
    await cache.delete(keys[i]);
  }
}

async function staleWhileRevalidate(event, request) {
  const cache = await caches.open(API_CACHE);
  const cached = await cache.match(request);

  const refresh = fetch(request)
    .then(async (response) => {
      if (response.ok) await cache.put(request, await stamp(response));
      return response;
    })
    .catch(() => undefined);

  if (cached && ageOf(cached) <= API_MAX_AGE_MS) {
    event.waitUntil(refresh);
    return cached;
  }

  const fresh = await refresh;
  if (fresh) return fresh;
  if (cached) return cached; // offline: stale beats nothing
  return new Response(JSON.stringify({ error: "offline" }), {
    status: 503,
    headers: { "Content-Type": "application/json" },
  });
}

// ---------------------------------------------------------------------------
// Fetch routing
// ---------------------------------------------------------------------------

self.addEventListener("fetch", (event) => {
  const { request } = event;
  const url = new URL(request.url);

  if (request.method !== "GET") {
    if (isApi(url)) event.respondWith(networkOrQueue(request));
    return;
  }

  if (request.destination === "image") {
    event.respondWith(cacheFirst(request, IMAGE_CACHE, IMAGE_MAX_AGE_MS));
  } else if (url.origin === self.location.origin && isStaticAsset(url)) {
    event.respondWith(cacheFirst(request, STATIC_CACHE));
  } else if (isApi(url)) {
    event.respondWith(staleWhileRevalidate(event, request));
  }
});

// ---------------------------------------------------------------------------
// Background sync for failed mutations
// ---------------------------------------------------------------------------

function openDb() {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(DB_NAME, 1);
    open.onupgradeneeded = () => {
      open.result.createObjectStore(STORE, { keyPath: "id", autoIncrement: true });
    };
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}

function withStore(mode, run) {
  return openDb().then(
    (db) =>
      new Promise((resolve, reject) => {
        const tx = db.transaction(STORE, mode);
        const result = run(tx.objectStore(STORE));
        tx.oncomplete = () => resolve(result && result.result);
        tx.onerror = () => reject(tx.error);
      }),
  );
}

async function networkOrQueue(request) {
  const replayable = request.clone();
  try {
    return await fetch(request);
  } catch (error) {
    const record = {
      url: replayable.url,
      method: replayable.method,
      headers: Array.from(replayable.headers.entries()),
      body: await replayable.text(),
      queuedAt: Date.now(),
    };
    await withStore("readwrite", (store) => store.add(record));
    if (self.registration.sync) {
      await self.registration.sync.register(SYNC_TAG);
    }
    await notifyClients({ type: "MUTATION_QUEUED" });
    return new Response(JSON.stringify({ queued: true }), {
      status: 202,
      headers: { "Content-Type": "application/json" },
    });
  }
}

async function replayQueuedMutations() {
  const queued = await withStore("readonly", (store) => store.getAll());
  let replayed = 0;
  for (const record of queued || []) {
    try {
      const response = await fetch(record.url, {
        method: record.method,
        headers: record.headers,
        body: record.body || undefined,
      });
      if (response.ok || (response.status >= 400 && response.status < 500)) {
        await withStore("readwrite", (store) => store.delete(record.id));
        replayed += 1;
      } else {
        throw new Error(`replay failed with ${response.status}`);
      }
    } catch (error) {
      throw error; // let Background Sync retry later
    }
  }
  if (replayed > 0) await notifyClients({ type: "MUTATIONS_REPLAYED", count: replayed });
}

self.addEventListener("sync", (event) => {
  if (event.tag === SYNC_TAG) event.waitUntil(replayQueuedMutations());
});

async function notifyClients(message) {
  const clients = await self.clients.matchAll({ includeUncontrolled: true });
  clients.forEach((client) => client.postMessage(message));
}
