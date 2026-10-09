import { Check, Minus, Plus, WandSparkles } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { allocate, parseWeight } from '@/features/ledger/expense/split'
import { useMoney, who } from '@/features/ledger/format'
import type { SplitKind, UserBrief } from '@/lib/api-types'
import { parseMoney, toMoneyInput } from '@/lib/money'
import { cn } from '@/lib/utils'

function Person({ person, myId, dim }: { person: UserBrief; myId: number; dim?: boolean }) {
  return (
    <span className={cn('flex min-w-0 flex-1 items-center gap-3', dim && 'opacity-50')}>
      <UserAvatar username={person.username} src={person.avatar_url} size="sm" />
      <span className="truncate text-base">{who(person, myId)}</span>
    </span>
  )
}

interface SplitEditorProps {
  people: UserBrief[]
  myId: number
  split: SplitKind
  /** Who shares it evenly (`equal`). */
  sharedBy: Set<number>
  onSharedBy: (next: Set<number>) => void
  /** Shares or exact amounts as typed (`shares`, `exact`); blank or zero: not in it. */
  weights: Map<number, string>
  onWeights: (next: Map<number, string>) => void
  /** Each person's part right now, as the server will compute it. */
  owed: Map<number, number>
  total: number
  currency: string
}

/**
 * One row per person with their part beside them, so the split reads at a glance: tap a row to
 * put them in or out (evenly), step their shares (a couple is 2), or type their exact amount.
 */
export function SplitEditor({
  people,
  myId,
  split,
  sharedBy,
  onSharedBy,
  weights,
  onWeights,
  owed,
  total,
  currency,
}: SplitEditorProps) {
  const money = useMoney()
  const exponent = money.exponent(currency)
  const part = (id: number) => (owed.has(id) ? money.format(owed.get(id) ?? 0, currency) : '—')

  if (split === 'equal') {
    return (
      <ul className="flex flex-col" aria-label="Kto się dzieli">
        {people.map((p) => {
          const on = sharedBy.has(p.id)
          return (
            <li key={p.id}>
              <button
                type="button"
                aria-pressed={on}
                onClick={() => {
                  const next = new Set(sharedBy)
                  if (on) next.delete(p.id)
                  else next.add(p.id)
                  onSharedBy(next)
                }}
                className="flex min-h-14 w-full items-center gap-3 rounded-lg px-2 text-left transition-colors hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
              >
                <span
                  aria-hidden
                  className={cn(
                    'flex size-6 shrink-0 items-center justify-center rounded-md border-2 transition-colors',
                    on ? 'border-primary bg-primary text-primary-foreground' : 'border-input',
                  )}
                >
                  {on && <Check className="size-4" strokeWidth={3} />}
                </span>
                <Person person={p} myId={myId} dim={!on} />
                <span
                  className={cn(
                    'text-base tabular-nums',
                    on ? 'font-semibold' : 'text-muted-foreground',
                  )}
                >
                  {part(p.id)}
                </span>
              </button>
            </li>
          )
        })}
      </ul>
    )
  }

  if (split === 'shares') {
    return (
      <ul className="flex flex-col" aria-label="Udziały">
        {people.map((p) => {
          const n = parseWeight(weights.get(p.id) ?? '')
          const set = (next: number) =>
            onWeights(new Map(weights).set(p.id, next > 0 ? String(next) : ''))
          return (
            <li key={p.id} className="flex min-h-14 items-center gap-3 px-2">
              <Person person={p} myId={myId} dim={n === 0} />
              <span className="text-sm text-muted-foreground tabular-nums">{part(p.id)}</span>
              <span
                className="flex items-center gap-1"
                role="group"
                aria-label={`Udziały: ${p.username}`}
              >
                <Button
                  type="button"
                  variant="outline"
                  size="icon-sm"
                  aria-label={`Mniej udziałów: ${p.username}`}
                  disabled={n === 0}
                  onClick={() => set(n - 1)}
                >
                  <Minus />
                </Button>
                <span
                  className="w-7 text-center text-lg font-semibold tabular-nums"
                  aria-live="polite"
                >
                  {n}
                </span>
                <Button
                  type="button"
                  variant="outline"
                  size="icon-sm"
                  aria-label={`Więcej udziałów: ${p.username}`}
                  onClick={() => set(n + 1)}
                >
                  <Plus />
                </Button>
              </span>
            </li>
          )
        })}
      </ul>
    )
  }

  // exact amounts: what is left is spread over the empty rows as their placeholders
  const assigned = [...weights.values()].reduce((sum, t) => sum + (parseMoney(t, exponent) ?? 0), 0)
  const leftover = total - assigned
  const empty = people.filter((p) => !(weights.get(p.id) ?? '').trim())
  const hints = new Map(
    leftover > 0
      ? allocate(
          leftover,
          empty.map(() => 1),
        ).map((minor, i) => [empty[i].id, minor] as const)
      : [],
  )
  return (
    <>
      <ul className="flex flex-col" aria-label="Kwoty">
        {people.map((p) => (
          <li key={p.id} className="flex min-h-14 items-center gap-3 px-2">
            <Person person={p} myId={myId} dim={!(weights.get(p.id) ?? '').trim()} />
            <Input
              aria-label={`Kwota: ${p.username}`}
              className="w-32 text-right tabular-nums"
              inputMode="decimal"
              placeholder={toMoneyInput(hints.get(p.id) ?? 0, exponent)}
              value={weights.get(p.id) ?? ''}
              onChange={(e) => onWeights(new Map(weights).set(p.id, e.target.value))}
            />
          </li>
        ))}
      </ul>
      <div className="flex flex-col items-start gap-1 px-2">
        <span className={cn('text-sm', leftover !== 0 && 'text-destructive')} aria-live="polite">
          {leftover === 0
            ? `Rozdzielono całe ${money.format(total, currency)}`
            : leftover > 0
              ? `Zostało do rozdzielenia ${money.format(leftover, currency)}`
              : `O ${money.format(-leftover, currency)} za dużo`}
        </span>
        {hints.size > 0 && (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="-ml-3"
            onClick={() => {
              const next = new Map(weights)
              for (const [id, minor] of hints) {
                if (minor > 0) next.set(id, toMoneyInput(minor, exponent))
              }
              onWeights(next)
            }}
          >
            Rozdziel resztę po równo
            <WandSparkles data-icon="inline-end" aria-hidden />
          </Button>
        )}
      </div>
    </>
  )
}
