import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { FormSection } from '@/features/ledger/form/controls'
import { PersonSelect } from '@/features/ledger/form/PersonSelect'
import { useMoney, who } from '@/features/ledger/format'
import type { UserBrief } from '@/lib/api-types'
import { toMoneyInput } from '@/lib/money'
import { cn } from '@/lib/utils'

interface PayersFieldProps {
  title: string
  people: UserBrief[]
  myId: number
  /** One person paid (received) everything... */
  payer: number | null
  onPayer: (id: number) => void
  /** ...or several, with what each of them did (null: one person). */
  payers: Map<number, string> | null
  onPayers: (next: Map<number, string> | null) => void
  total: number
  paid: number
  currency: string
}

/** Who paid for an expense (who received an income): usually one person, sometimes several. */
export function PayersField({
  title,
  people,
  myId,
  payer,
  onPayer,
  payers,
  onPayers,
  total,
  paid,
  currency,
}: PayersFieldProps) {
  const money = useMoney()
  const exponent = money.exponent(currency)
  const toggle =
    payers === null ? (
      <Button
        type="button"
        variant="ghost"
        size="sm"
        onClick={() =>
          onPayers(new Map(payer !== null ? [[payer, toMoneyInput(total, exponent)]] : []))
        }
      >
        Kilka osób
      </Button>
    ) : (
      <Button type="button" variant="ghost" size="sm" onClick={() => onPayers(null)}>
        Jedna osoba
      </Button>
    )

  return (
    <FormSection title={title} action={toggle}>
      {payers === null ? (
        <PersonSelect
          aria-label={title}
          people={people}
          myId={myId}
          value={payer}
          onChange={onPayer}
        />
      ) : (
        <>
          <ul className="flex flex-col gap-2">
            {people.map((p) => (
              <li key={p.id} className="flex items-center gap-3">
                <UserAvatar username={p.username} src={p.avatar_url} size="sm" />
                <span className="min-w-0 flex-1 truncate text-base">{who(p, myId)}</span>
                <Input
                  aria-label={`${title}: ${p.username}`}
                  className="w-32 text-right tabular-nums"
                  inputMode="decimal"
                  placeholder="0"
                  value={payers.get(p.id) ?? ''}
                  onChange={(e) => onPayers(new Map(payers).set(p.id, e.target.value))}
                />
              </li>
            ))}
          </ul>
          <span className={cn('text-sm', paid !== total && 'text-destructive')}>
            Razem {money.format(paid, currency)} z {money.format(total, currency)}
          </span>
        </>
      )}
    </FormSection>
  )
}
