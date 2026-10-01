import { Plus, X } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

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
}: {
  value: string[]
  onChange: (value: string[]) => void
  error?: string
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
            disabled={value.length === 1}
            onClick={() => onChange(value.filter((_, j) => j !== i))}
          >
            <X className="size-5" aria-hidden />
          </Button>
        </div>
      ))}
      <div>
        <Button type="button" variant="outline" onClick={() => onChange([...value, ''])}>
          <Plus className="size-5" aria-hidden /> Dodaj kwotę
        </Button>
      </div>
      {error && (
        <span role="alert" className="text-sm text-destructive">
          {error}
        </span>
      )}
    </fieldset>
  )
}
