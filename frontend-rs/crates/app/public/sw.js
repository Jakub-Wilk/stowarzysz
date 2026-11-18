// Service worker: Web Push only (a port of frontend/src/sw.ts). It deliberately has no fetch
// handler and caches nothing, so every load comes from the server (index.html is revalidated,
// /assets/ are content-hashed). Don't add caching here.

self.addEventListener('install', () => {
  self.skipWaiting()
})

// Earlier versions precached the app shell; drop those caches so nobody stays on an old build.
self.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys()
      await Promise.all(keys.map((key) => caches.delete(key)))
      await self.clients.claim()
    })(),
  )
})

self.addEventListener('push', (event) => {
  let payload = {}
  try {
    payload = event.data ? event.data.json() : {}
  } catch {
    payload = {}
  }
  event.waitUntil(
    self.registration.showNotification(payload.title || 'stowarzysz', {
      body: payload.body,
      icon: '/pwa-192x192.png',
      badge: '/pwa-192x192.png',
      data: { url: payload.url || '/' },
    }),
  )
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const data = event.notification.data || {}
  const target = new URL(data.url || '/', self.location.origin).href
  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({ type: 'window', includeUncontrolled: true })
      const open = windows.find((w) => new URL(w.url).origin === self.location.origin)
      if (open) {
        await open.focus()
        await open.navigate(target)
      } else {
        await self.clients.openWindow(target)
      }
    })(),
  )
})
