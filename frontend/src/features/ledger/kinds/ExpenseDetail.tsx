import { UserAvatar } from '@/features/auth/UserAvatar'
import { formatDay, useMoney, who } from '@/features/ledger/format'
import { isItemized, peopleOf } from '@/features/ledger/kinds/helpers'
import type { ExpenseEntry, ExpenseItem, UserBrief } from '@/lib/api-types'
import { formatMoney } from '@/lib/money'
import { cn } from '@/lib/utils'

const SPLIT_LABEL = { equal: 'po równo', shares: 'udziałami', exact: 'kwotami' } as const

function Heading({ children }: { children: string }) {
  return (
    <h3 className="text-sm font-semibold tracking-wide text-muted-foreground uppercase">
      {children}
    </h3>
  )
}

interface ItemRowProps {
  item: ExpenseItem
  people: Map<number, UserBrief>
  currency: string
}

function ItemRow({ item, people, currency }: ItemRowProps) {
  const money = useMoney()
  return (
    <li className="flex items-center gap-3 py-2">
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="truncate text-base font-medium">{item.name}</span>
        <span className="flex items-center gap-1">
          {item.shares.map((s) => {
            const person = people.get(s.user_id)
            return person ? (
              <UserAvatar
                key={s.user_id}
                username={person.username}
                src={person.avatar_url}
                size="sm"
              />
            ) : null
          })}
          {item.split !== 'equal' && (
            <span className="ml-1 text-xs text-muted-foreground">{SPLIT_LABEL[item.split]}</span>
          )}
        </span>
      </div>
      <span className="text-base font-semibold tabular-nums">
        {money.format(item.amount, currency)}
      </span>
    </li>
  )
}

export function ExpenseDetail({ entry, myId }: { entry: ExpenseEntry; myId: number | undefined }) {
  const money = useMoney()
  const people = peopleOf(entry)
  const foreign = entry.rate !== null
  const rate = Number(entry.rate).toLocaleString('pl-PL', { maximumFractionDigits: 4 })

  return (
    <>
      {foreign && (
        <p className="rounded-lg border bg-card px-4 py-3 text-base">
          Razem {money.format(entry.amount ?? 0, entry.currency)} ≈{' '}
          <strong>{formatMoney(entry.base_amount ?? 0)}</strong>
          <span className="block text-sm text-muted-foreground">
            1 {entry.currency} = {rate} zł, kurs EBC z {formatDay(entry.rate_date ?? '')}
          </span>
        </p>
      )}

      {isItemized(entry) && (
        <section className="flex flex-col gap-1 rounded-lg border bg-card px-4 py-3">
          <Heading>Pozycje</Heading>
          <ul className="divide-y">
            {entry.details.items.map((item, i) => (
              <ItemRow key={i} item={item} people={people} currency={entry.currency} />
            ))}
          </ul>
        </section>
      )}

      <section className="flex flex-col gap-1 rounded-lg border bg-card px-4 py-3">
        <Heading>
          {!isItemized(entry) && entry.details.items[0]?.split !== 'equal'
            ? `Podział ${SPLIT_LABEL[entry.details.items[0].split]}`
            : 'Podział'}
        </Heading>
        <ul className="divide-y">
          {entry.breakdown.map((row) => (
            <li key={row.user.id} className="flex items-center gap-3 py-2">
              <UserAvatar username={row.user.username} src={row.user.avatar_url} size="md" />
              <div className="flex min-w-0 flex-1 flex-col">
                <span className={cn('truncate text-base', row.user.id === myId && 'font-semibold')}>
                  {who(row.user, myId)}
                </span>
                {row.paid > 0 && (
                  <span className="text-sm text-muted-foreground">
                    zapłacił(a) {money.format(row.paid, entry.currency)}
                  </span>
                )}
              </div>
              <span className="flex flex-col items-end">
                <span className="text-base font-semibold tabular-nums">
                  {money.format(row.owed, entry.currency)}
                </span>
                {foreign && (
                  <span className="text-xs text-muted-foreground tabular-nums">
                    ≈ {formatMoney(row.owed_base)}
                  </span>
                )}
              </span>
            </li>
          ))}
        </ul>
      </section>
    </>
  )
}
