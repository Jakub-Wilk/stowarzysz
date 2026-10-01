import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiFetch } from '@/lib/api'

const supported = () =>
  typeof window !== 'undefined' &&
  'serviceWorker' in navigator &&
  'PushManager' in window &&
  'Notification' in window

/** Base64url VAPID key -> the bytes `pushManager.subscribe` wants. */
function keyBytes(base64Url: string): Uint8Array<ArrayBuffer> {
  const padded = base64Url
    .replace(/-/g, '+')
    .replace(/_/g, '/')
    .padEnd(Math.ceil(base64Url.length / 4) * 4, '=')
  const raw = atob(padded)
  const bytes = new Uint8Array(new ArrayBuffer(raw.length))
  for (let i = 0; i < raw.length; i += 1) bytes[i] = raw.charCodeAt(i)
  return bytes
}

async function currentSubscription(): Promise<PushSubscription | null> {
  const registration = await navigator.serviceWorker.ready
  return registration.pushManager.getSubscription()
}

export type PushState = 'unsupported' | 'unconfigured' | 'blocked' | 'off' | 'on' | 'loading'

/** Notification opt-in for this browser: server key, permission and subscription in one place. */
export function usePush() {
  const queryClient = useQueryClient()
  const isSupported = supported()

  const key = useQuery({
    queryKey: ['push', 'key'],
    queryFn: () => apiFetch<{ public_key: string | null }>('/api/push/public-key/'),
    enabled: isSupported,
    staleTime: Infinity,
  })
  const subscription = useQuery({
    queryKey: ['push', 'subscription'],
    queryFn: async () => (await currentSubscription())?.endpoint ?? null,
    enabled: isSupported,
  })

  const refresh = () => queryClient.invalidateQueries({ queryKey: ['push', 'subscription'] })

  const enable = useMutation({
    mutationFn: async () => {
      const publicKey = key.data?.public_key
      if (!publicKey) throw new Error('Notifications are not configured on the server.')
      // Must happen in response to a click: browsers ignore prompts that aren't.
      if ((await Notification.requestPermission()) !== 'granted') return
      const registration = await navigator.serviceWorker.ready
      const sub = await registration.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: keyBytes(publicKey),
      })
      await apiFetch('/api/push/subscriptions/', { method: 'POST', json: sub.toJSON() })
    },
    onSettled: refresh,
  })

  const disable = useMutation({
    mutationFn: async () => {
      const sub = await currentSubscription()
      if (!sub) return
      await apiFetch('/api/push/subscriptions/', {
        method: 'DELETE',
        json: { endpoint: sub.endpoint },
      })
      await sub.unsubscribe()
    },
    onSettled: refresh,
  })

  let state: PushState
  if (!isSupported) state = 'unsupported'
  else if (key.isPending || subscription.isPending) state = 'loading'
  else if (!key.data?.public_key) state = 'unconfigured'
  else if (subscription.data) state = 'on'
  else if (Notification.permission === 'denied') state = 'blocked'
  else state = 'off'

  return { state, enable, disable, busy: enable.isPending || disable.isPending }
}
