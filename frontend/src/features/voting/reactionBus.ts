import type { ReactionEvent } from '@/lib/api-types'

type Listener = (event: ReactionEvent) => void

const listeners = new Set<Listener>()

/** Tiny pub/sub between the SSE stream and whichever results screen is open. */
export function subscribeReactions(listener: Listener): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function emitReaction(event: ReactionEvent): void {
  listeners.forEach((listener) => listener(event))
}
