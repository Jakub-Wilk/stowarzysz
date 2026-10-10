import { useCallback, useEffect, useRef, useState } from 'react'

import { subscribeOcr } from '@/features/ledger/expense/ocrBus'
import { apiFetch } from '@/lib/api'
import type { ScannedReceipt } from '@/lib/api-types'

/** The bar has four quarters; the server sends at most three requests, the last quarter is the result. */
export const QUARTERS = 4
/** A quarter fills this long after the server says it sent a request. */
const STEP_DELAY_MS = 1000

/**
 * Reads a receipt photo on the server. Progress is in quarters: one more a second after each
 * `ledger.ocr.request` the server announces over SSE, all four the moment the result arrives.
 */
export function useReceiptScan() {
  const [quarters, setQuarters] = useState(0)
  const [pending, setPending] = useState(false)
  const timers = useRef(new Set<number>())
  const unsubscribe = useRef<(() => void) | null>(null)

  const stop = useCallback(() => {
    timers.current.forEach((timer) => window.clearTimeout(timer))
    timers.current.clear()
    unsubscribe.current?.()
    unsubscribe.current = null
  }, [])

  useEffect(() => stop, [stop])

  const scan = useCallback(
    async (image: File): Promise<ScannedReceipt> => {
      stop()
      const jobId = crypto.randomUUID()
      setQuarters(0)
      setPending(true)
      unsubscribe.current = subscribeOcr((event) => {
        if (event.job_id !== jobId) return
        const timer = window.setTimeout(() => {
          timers.current.delete(timer)
          setQuarters((q) => Math.min(q + 1, QUARTERS - 1))
        }, STEP_DELAY_MS)
        timers.current.add(timer)
      })
      const form = new FormData()
      form.append('image', image)
      form.append('job_id', jobId)
      try {
        const receipt = await apiFetch<ScannedReceipt>('/api/ledger/receipt-ocr/', {
          method: 'POST',
          form,
        })
        stop()
        setQuarters(QUARTERS)
        return receipt
      } catch (error) {
        stop()
        setQuarters(0)
        throw error
      } finally {
        setPending(false)
      }
    },
    [stop],
  )

  return { scan, quarters, pending }
}
