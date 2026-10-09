import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Button } from '@/components/ui/button'
import type { PactKindUI } from '@/features/pacts/kinds/types'
import type { TermsState } from '@/features/pacts/terms'

interface SidePickerProps {
  sides: string[]
  value: string
  onChange: (side: string) => void
  disabled?: boolean
}

export function SidePicker({ sides, value, onChange, disabled }: SidePickerProps) {
  return (
    <div className="flex flex-wrap gap-2" role="group" aria-label="Strona">
      {sides.map((side) => (
        <Button
          key={side}
          type="button"
          variant={value === side ? 'default' : 'outline'}
          aria-pressed={value === side}
          disabled={disabled}
          onClick={() => onChange(side)}
        >
          {side}
        </Button>
      ))}
    </div>
  )
}

interface TermsFieldsProps {
  kind: PactKindUI
  /** The pact's sides (sided kinds). */
  sides: string[]
  state: TermsState
  onChange: (state: TermsState) => void
  /** Prefix for element ids when several forms share a page. */
  idPrefix: string
  disabled?: boolean
  /** Hide the amount (e.g. a bet's host, who just matches each opponent). */
  hideStake?: boolean
}

/** Side / stake inputs for whatever the kind asks of a person. */
export function TermsFields({
  kind,
  sides,
  state,
  onChange,
  idPrefix,
  disabled,
  hideStake,
}: TermsFieldsProps) {
  const set = (patch: Partial<TermsState>) => onChange({ ...state, ...patch })
  const showMoney = kind.money !== 'none' && !hideStake
  return (
    <div className="flex flex-col gap-4">
      {kind.sided && (
        <div className="flex flex-col gap-2">
          <Label>Twoja strona</Label>
          <SidePicker
            sides={sides}
            value={state.side}
            onChange={(side) => set({ side })}
            disabled={disabled}
          />
        </div>
      )}
      {showMoney && (
        <div className="flex flex-col gap-2">
          <Label htmlFor={`${idPrefix}-amount`}>
            {kind.money === 'pot' ? 'Stawka (zł, opcjonalnie)' : 'Stawka (zł)'}
          </Label>
          <Input
            id={`${idPrefix}-amount`}
            inputMode="decimal"
            placeholder="np. 10 lub 12,50"
            value={state.amount}
            disabled={disabled}
            onChange={(e) => set({ amount: e.target.value })}
          />
        </div>
      )}
      {!hideStake && kind.money !== 'none' && (
        <div className="flex flex-col gap-2">
          <Label htmlFor={`${idPrefix}-note`}>
            {kind.money === 'wager' ? 'Albo opis stawki' : 'Opis stawki (opcjonalnie)'}
          </Label>
          <span className="text-sm text-muted-foreground">
            Sama stawka opisowa (bez kwoty) po rozstrzygnięciu staje się długiem w Rozliczeniach.
          </span>
          <Input
            id={`${idPrefix}-note`}
            maxLength={200}
            placeholder="np. kolacja"
            value={state.note}
            disabled={disabled}
            onChange={(e) => set({ note: e.target.value })}
          />
        </div>
      )}
    </div>
  )
}
