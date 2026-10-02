import { Plus, X } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import type { SantaMode } from '@/lib/api-types'

export function DeadlineField({
  value,
  onChange,
  error,
}: {
  value: string
  onChange: (value: string) => void
  error?: string
}) {
  return (
    <div className="flex flex-col gap-2">
      <Label htmlFor="santa-deadline">Termin</Label>
      <Input
        id="santa-deadline"
        type="datetime-local"
        value={value}
        required
        aria-invalid={error !== undefined}
        onChange={(e) => onChange(e.target.value)}
      />
      <span className="text-sm text-muted-foreground">
        Termin jest tylko informacją. Secret Santa kończysz ręcznie.
      </span>
      {error && (
        <span role="alert" className="text-sm text-destructive">
          {error}
        </span>
      )}
    </div>
  )
}

/** Editable list of gift amounts in PLN; values stay strings until submitted. */
export function TiersField({
  value,
  onChange,
  error,
  fixedCount = false,
}: {
  value: string[]
  onChange: (value: string[]) => void
  error?: string
  /** The number of amounts can't change (a per-tier draw already exists). */
  fixedCount?: boolean
}) {
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="text-base font-medium">Kwoty prezentów (zł)</legend>
      {value.map((amount, i) => (
        <div key={i} className="flex items-center gap-2">
          <Input
            type="number"
            inputMode="numeric"
            min={1}
            step={1}
            value={amount}
            aria-label={`Kwota ${i + 1}`}
            aria-invalid={error !== undefined}
            onChange={(e) => onChange(value.map((v, j) => (j === i ? e.target.value : v)))}
          />
          <Button
            type="button"
            variant="ghost"
            size="icon"
            aria-label={`Usuń kwotę ${i + 1}`}
            disabled={fixedCount || value.length === 1}
            onClick={() => onChange(value.filter((_, j) => j !== i))}
          >
            <X className="size-5" aria-hidden />
          </Button>
        </div>
      ))}
      {!fixedCount && (
        <div>
          <Button type="button" variant="outline" onClick={() => onChange([...value, ''])}>
            <Plus className="size-5" aria-hidden /> Dodaj kwotę
          </Button>
        </div>
      )}
      {error && (
        <span role="alert" className="text-sm text-destructive">
          {error}
        </span>
      )}
    </fieldset>
  )
}

const MODES: { value: SantaMode; title: string; hint: string }[] = [
  {
    value: 'single',
    title: 'Jeden podopieczny',
    hint: 'Każdy obdarowuje jedną osobę wszystkimi kwotami.',
  },
  {
    value: 'per_tier',
    title: 'Podopieczny na kwotę',
    hint: 'Każda kwota to inna osoba, więc każdy obdarowuje tyle osób, ile jest kwot.',
  },
]

/** How the draw is split across the gift amounts. */
export function ModeField({
  value,
  onChange,
}: {
  value: SantaMode
  onChange: (value: SantaMode) => void
}) {
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="text-base font-medium">Tryb losowania</legend>
      {MODES.map((mode) => (
        <label
          key={mode.value}
          className="flex cursor-pointer items-start gap-3 rounded-lg border bg-card px-4 py-3 transition-colors hover:bg-accent has-[:checked]:border-primary"
        >
          <input
            type="radio"
            name="santa-mode"
            className="mt-1.5"
            checked={value === mode.value}
            onChange={() => onChange(mode.value)}
          />
          <span className="flex flex-col">
            <span className="text-base font-medium">{mode.title}</span>
            <span className="text-sm text-muted-foreground">{mode.hint}</span>
          </span>
        </label>
      ))}
    </fieldset>
  )
}
