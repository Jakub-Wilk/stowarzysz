import type { LucideIcon } from 'lucide-react'
import type { ComponentType } from 'react'

import type { LedgerEntry } from '@/lib/api-types'

/**
 * What the UI needs to know about one kind of ledger entry (mirrors `ledger/kinds.py`). The
 * generic screens never look inside an entry's `details`, so a new kind is one file here plus
 * an entry in `index.ts`. What the user may do comes from the server's `actions`, not from here.
 */
export interface LedgerKindUI<E extends LedgerEntry = LedgerEntry> {
  key: E['kind']
  /** Short name for chips. */
  label: string
  icon: LucideIcon
  /** How the entry's status reads (Polish nouns have genders: wpłata / zwrot / wydatek). */
  statusLabel: (entry: E) => string
  /** One line under the title in the feed, e.g. "zapłacił(a) alice". */
  summary: (entry: E, myId: number | undefined) => string
  /** Button labels for the actions the server allows. */
  actionLabels: (entry: E) => { confirm: string; reject: string; cancel: string }
  /** It settles debts rather than creating them (a payment): what it means for you reads
   * neutral, not as a loss for the person paid back. */
  settles?: boolean
  /** Ask before `cancel` (it can't be undone)? Then this is the question. */
  cancelQuestion?: string
  /** The kind-specific part of the detail page. */
  Detail: ComponentType<{ entry: E; myId: number | undefined }>
}
