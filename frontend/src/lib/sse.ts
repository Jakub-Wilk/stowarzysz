import { fetchEventSource } from '@microsoft/fetch-event-source'

import { authorizedFetch } from '@/lib/api'

export type EventHandler = (type: string, data: unknown) => void

class FatalStreamError extends Error {}

/**
 * Opens the per-user SSE stream. Native EventSource can't send an Authorization header, so
 * this uses a fetch-based client with `authorizedFetch` (Bearer token, refresh on 401).
 * The server picks the channel from the authenticated user. Abort `signal` to close.
 */
export function connectEventStream(onEvent: EventHandler, signal: AbortSignal): Promise<void> {
  return fetchEventSource('/api/events/', {
    signal,
    fetch: authorizedFetch,
    openWhenHidden: true,
    async onopen(res) {
      if (res.ok) return
      // 4xx other than a timeout/rate limit won't fix itself by retrying (e.g. session lost).
      if (res.status >= 400 && res.status < 500 && res.status !== 408 && res.status !== 429) {
        throw new FatalStreamError(`event stream rejected: ${res.status}`)
      }
      throw new Error(`event stream failed: ${res.status}`)
    },
    onmessage(message) {
      let data: unknown = message.data
      try {
        data = JSON.parse(message.data)
      } catch {
        // non-JSON payloads are passed through as strings
      }
      onEvent(message.event || 'message', data)
    },
    onerror(error) {
      if (error instanceof FatalStreamError) throw error // stop; otherwise retry with backoff
    },
  })
}
