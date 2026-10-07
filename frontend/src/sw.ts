/// <reference lib="webworker" />
// Service worker: Web Push only. It deliberately has no fetch handler and caches nothing, so
// every load comes from the server (index.html is revalidated, /assets/ are content-hashed).

declare const self: ServiceWorkerGlobalScope

self.addEventListener('install', () => {
  void self.skipWaiting()
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

interface PushPayload {
  title?: string
  body?: string
  url?: string
}

self.addEventListener('push', (event) => {
  const payload: PushPayload = event.data?.json() ?? {}
  event.waitUntil(
    self.registration.showNotification(payload.title ?? 'stowarzysz', {
      body: payload.body,
      icon: '/pwa-192x192.png',
      badge: '/pwa-192x192.png',
      data: { url: payload.url ?? '/' },
    }),
  )
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const target = new URL(
    (event.notification.data as { url?: string } | null)?.url ?? '/',
    self.location.origin,
  ).href
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
