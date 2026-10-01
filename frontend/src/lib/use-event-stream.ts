import { useEffect, useRef } from 'react'

import { useHasSession } from '@/features/auth/hooks'
import { connectEventStream, type EventHandler } from '@/lib/sse'

/** Subscribes to the user's SSE stream while signed in; reconnects are handled internally. */
export function useEventStream(onEvent: EventHandler) {
  const isAuthenticated = useHasSession()
  const handler = useRef(onEvent)

  useEffect(() => {
    handler.current = onEvent
  })

  useEffect(() => {
    if (!isAuthenticated) return
    const controller = new AbortController()
    connectEventStream((type, data) => handler.current(type, data), controller.signal).catch(
      () => undefined, // fatal rejection (e.g. session lost); a new login remounts this effect
    )
    return () => controller.abort()
  }, [isAuthenticated])
}
