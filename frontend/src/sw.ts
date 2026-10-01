/// <reference lib="webworker" />
// Service worker: offline shell (same as the generated one used to do) plus Web Push.
import { clientsClaim } from 'workbox-core'
import {
  cleanupOutdatedCaches,
  createHandlerBoundToURL,
  precacheAndRoute,
} from 'workbox-precaching'
import { NavigationRoute, registerRoute } from 'workbox-routing'

declare const self: ServiceWorkerGlobalScope

void self.skipWaiting()
clientsClaim()

precacheAndRoute(self.__WB_MANIFEST)
cleanupOutdatedCaches()
// Single-page app navigation, but never for the API or the SSE stream. In dev the precache
// manifest is empty, so index.html isn't precached and there is no shell to serve.
try {
  registerRoute(
    new NavigationRoute(createHandlerBoundToURL('index.html'), { denylist: [/^\/api/] }),
  )
} catch {
  // no precached shell (dev server): navigations go to the network
}

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
