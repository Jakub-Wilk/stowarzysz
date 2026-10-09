import { Link } from 'react-router'

import { useFlashOnChange } from '@/components/motion/useFlashOnChange'
import { EntryActions } from '@/features/ledger/EntryActions'
import { formatImpact, impactOf, impactTone, useCategory, useMoney } from '@/features/ledger/format'
import { kindOf } from '@/features/ledger/kinds'
import { toneClasses } from '@/features/voting/tone'
import type { LedgerEntry, LedgerEntryStatus, Tone } from '@/lib/api-types'
import { cn } from '@/lib/utils'

const STATUS_TONE: Record<LedgerEntryStatus, Tone> = {
  pending: 'neutral',
  confirmed: 'positive',
  rejected: 'negative',
  cancelled: 'neutral',
}

export function EntryStatusChip({ entry }: { entry: LedgerEntry }) {
  return (
    <span
      className={cn(
        'rounded-full px-2.5 py-0.5 text-xs font-bold uppercase',
        toneClasses(STATUS_TONE[entry.status]).chip,
      )}
    >
      {kindOf(entry).statusLabel(entry)}
    </span>
  )
}

/** One entry in the feed: what it was, who was involved, and what it means for you. A payment
 * waiting for you carries its buttons, so you can answer without opening it. */
export function EntryRow({ entry, myId }: { entry: LedgerEntry; myId: number | undefined }) {
  const kind = kindOf(entry)
  const money = useMoney()
  const flashing = useFlashOnChange(`${entry.status}:${entry.version}`)
  const impact = impactOf(entry, myId)
  const mine = impact ? formatImpact(impact) : null
  const headline = money.headline(entry)
  const Icon = kind.icon
  const category = useCategory()(entry)

  return (
    <div
      className={cn(
        'flex flex-col gap-3 rounded-lg border bg-card transition-transform duration-(--duration-base) ease-(--ease-out-soft) hover:-translate-y-0.5',
        entry.status !== 'confirmed' && entry.status !== 'pending' && 'opacity-60',
        entry.status === 'pending' && 'border-dashed',
        flashing && 'animate-flash',
      )}
    >
      <Link
        to={`/ledger/${entry.id}`}
        className="flex items-center gap-4 rounded-lg p-4 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
      >
        <span className="flex size-11 shrink-0 items-center justify-center rounded-full bg-muted">
          {category ? (
            <span role="img" aria-label={category.label} className="text-2xl leading-none">
              {category.emoji}
            </span>
          ) : (
            <Icon className="size-5" aria-label={kind.label} />
          )}
        </span>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="text-metal truncate text-lg leading-snug font-semibold">
            {entry.title}
          </span>
          <span className="truncate text-sm text-muted-foreground">
            {kind.summary(entry, myId)}
          </span>
          {entry.status !== 'confirmed' && (
            <span className="mt-1">
              <EntryStatusChip entry={entry} />
            </span>
          )}
        </div>
        <span className="flex shrink-0 flex-col items-end gap-0.5">
          {headline && <span className="text-lg font-bold tabular-nums">{headline}</span>}
          {impact && mine && (
            <span
              className={cn(
                'text-sm tabular-nums',
                toneClasses(kind.settles ? 'neutral' : impactTone(impact)).text,
              )}
            >
              {mine}
            </span>
          )}
        </span>
      </Link>
      {entry.status === 'pending' && (
        <div className="px-4 pb-4 empty:hidden">
          <EntryActions entry={entry} />
        </div>
      )}
    </div>
  )
}
