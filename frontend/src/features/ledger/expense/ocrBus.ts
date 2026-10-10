import type { OcrRequestEvent } from '@/lib/api-types'

type Listener = (event: OcrRequestEvent) => void

const listeners = new Set<Listener>()

/** Tiny pub/sub between the SSE stream and the receipt scan in the open expense form. */
export function subscribeOcr(listener: Listener): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function emitOcr(event: OcrRequestEvent): void {
  listeners.forEach((listener) => listener(event))
}
