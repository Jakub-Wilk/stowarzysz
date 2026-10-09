import { Plus, X } from 'lucide-react'
import { useState, type SubmitEvent } from 'react'
import { useNavigate } from 'react-router'

import { BackLink } from '@/components/layout/BackLink'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import { useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { endOfDayIso, tomorrowInput } from '@/features/pacts/format'
import { useCreatePact } from '@/features/pacts/hooks'
import { getPactKind, PACT_KINDS } from '@/features/pacts/kinds'
import { TermsFields } from '@/features/pacts/TermsFields'
import { buildTerms, emptyTerms, type TermsState } from '@/features/pacts/terms'
import { usePeople } from '@/features/voting/hooks'
import { formErrors } from '@/lib/api-errors'
import type { PactInvitee, PactKindKey } from '@/lib/api-types'
import { cn } from '@/lib/utils'

const MAX_SIDES = 6
const DEFAULT_SIDES = ['tak', 'nie']

function FieldError({ message }: { message?: string }) {
  return message ? (
    <span role="alert" className="text-base text-destructive">
      {message}
    </span>
  ) : null
}

/** Propose a pact: pick the kind, describe it, set your own terms and invite people. */
export function PactNewPage() {
  const navigate = useNavigate()
  const { data: me } = useMe()
  const people = usePeople()
  const create = useCreatePact()

  const [kindKey, setKindKey] = useState<PactKindKey>('bet')
  const [title, setTitle] = useState('')
  const [condition, setCondition] = useState('')
  const [notes, setNotes] = useState('')
  const [due, setDue] = useState('')
  const [isOpen, setIsOpen] = useState(false)
  const [sides, setSides] = useState<string[]>(DEFAULT_SIDES)
  const [host, setHost] = useState<TermsState>(emptyTerms)
  const [invited, setInvited] = useState<ReadonlyMap<number, TermsState>>(new Map())
  const [localError, setLocalError] = useState<string | null>(null)

  const kind = getPactKind(kindKey)
  const others = (people.data ?? []).filter((p) => p.id !== me?.id)
  const cleanSides = sides.map((s) => s.trim())
  const errors = formErrors(create.error)

  const toggle = (id: number) =>
    setInvited((current) => {
      const next = new Map(current)
      if (!next.delete(id)) next.set(id, emptyTerms)
      return next
    })
  const setTerms = (id: number, terms: TermsState) =>
    setInvited((current) => new Map(current).set(id, terms))

  const pickKind = (key: PactKindKey) => {
    setKindKey(key)
    setLocalError(null)
    create.reset()
    if (!getPactKind(key).canBeOpen) setIsOpen(false)
  }

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (create.isPending) return
    setLocalError(null)
    if (kind.needsDue && !due) return setLocalError('Podaj termin.')
    if (
      kind.sided &&
      (cleanSides.some((s) => !s) || new Set(cleanSides).size !== cleanSides.length)
    ) {
      return setLocalError('Strony muszą być niepuste i różne.')
    }

    // a bet's host just matches each opponent; sided and pot kinds need the host's own terms
    let hostTerms
    if (kind.sided || kind.money === 'pot') {
      const built = buildTerms(kind, host)
      if (built.error !== null) return setLocalError(`Twoje warunki: ${built.error}`)
      hostTerms = built.terms
    }

    const opponents: PactInvitee[] = []
    for (const [userId, state] of invited) {
      const name = others.find((p) => p.id === userId)?.username ?? 'zaproszona osoba'
      if (kind.money === 'wager') {
        const built = buildTerms(kind, state)
        if (built.error !== null) return setLocalError(`${name}: ${built.error}`)
        opponents.push({ user_id: userId, ...built.terms })
      } else {
        opponents.push({ user_id: userId })
      }
    }

    create.mutate(
      {
        kind: kindKey,
        title: title.trim(),
        condition: condition.trim(),
        notes: notes.trim(),
        due_at: due ? endOfDayIso(due) : null,
        is_open: isOpen,
        config: kind.sided ? { sides: cleanSides } : {},
        host: hostTerms,
        opponents,
      },
      { onSuccess: (pact) => navigate(`/pacts/${pact.id}`, { replace: true }) },
    )
  }

  return (
    <>
      <BackLink to="/pacts">Zakłady</BackLink>
      <h2 className="mb-6 text-2xl font-semibold">Nowy zakład</h2>

      <form onSubmit={submit} className="flex max-w-md flex-col gap-6">
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 text-base font-medium">Rodzaj</legend>
          <div className="grid grid-cols-2 gap-2">
            {PACT_KINDS.map((k) => (
              <Button
                key={k.key}
                type="button"
                variant={kindKey === k.key ? 'default' : 'outline'}
                aria-pressed={kindKey === k.key}
                onClick={() => pickKind(k.key)}
              >
                {k.label}
              </Button>
            ))}
          </div>
          <p className="text-sm text-muted-foreground">{kind.blurb}</p>
        </fieldset>

        <div className="flex flex-col gap-2">
          <Label htmlFor="title">Nazwa</Label>
          <Input
            id="title"
            value={title}
            maxLength={200}
            aria-invalid={errors.fields.title !== undefined}
            onChange={(e) => setTitle(e.target.value)}
            autoFocus
          />
          {create.isError && <FieldError message={errors.fields.title} />}
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="condition">
            {kind.key === 'resolution' ? 'Do czego się zobowiązujesz?' : 'Warunek'}
          </Label>
          <Textarea
            id="condition"
            value={condition}
            aria-invalid={errors.fields.condition !== undefined}
            placeholder={
              kind.key === 'bet'
                ? 'Co musi się stać, żebyś wygrał(a)?'
                : kind.key === 'resolution'
                  ? 'Co dokładnie zrobisz?'
                  : 'O co się zakładamy?'
            }
            onChange={(e) => setCondition(e.target.value)}
          />
          {create.isError && <FieldError message={errors.fields.condition} />}
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="due">{kind.needsDue ? 'Termin' : 'Termin (opcjonalnie)'}</Label>
          <Input
            id="due"
            type="date"
            min={tomorrowInput()}
            value={due}
            onChange={(e) => setDue(e.target.value)}
          />
        </div>

        {kind.sided && (
          <fieldset className="flex flex-col gap-2">
            <legend className="mb-1 text-base font-medium">Strony</legend>
            {sides.map((side, i) => (
              <div key={i} className="flex gap-2">
                <Input
                  aria-label={`Strona ${i + 1}`}
                  value={side}
                  maxLength={32}
                  onChange={(e) => setSides(sides.map((s, j) => (j === i ? e.target.value : s)))}
                />
                {sides.length > 2 && (
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    aria-label={`Usuń stronę ${i + 1}`}
                    onClick={() => setSides(sides.filter((_, j) => j !== i))}
                  >
                    <X />
                  </Button>
                )}
              </div>
            ))}
            {sides.length < MAX_SIDES && (
              <Button
                type="button"
                variant="ghost"
                className="self-start"
                onClick={() => setSides([...sides, ''])}
              >
                <Plus /> Dodaj stronę
              </Button>
            )}
          </fieldset>
        )}

        {(kind.sided || kind.money === 'pot') && (
          <fieldset className="flex flex-col gap-2">
            <legend className="mb-1 text-base font-medium">Twoje warunki</legend>
            <TermsFields
              kind={kind}
              sides={cleanSides.filter(Boolean)}
              state={host}
              onChange={setHost}
              idPrefix="host"
            />
          </fieldset>
        )}

        {kind.canBeOpen && (
          <label className="flex items-center justify-between gap-4 rounded-lg border bg-card px-4 py-3">
            <span className="flex flex-col">
              <span className="text-base font-medium">Otwarty</span>
              <span className="text-sm text-muted-foreground">
                Każdy może poprosić o dołączenie.
              </span>
            </span>
            <Switch checked={isOpen} onCheckedChange={setIsOpen} aria-label="Otwarty zakład" />
          </label>
        )}

        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 text-base font-medium">Zaproś</legend>
          {people.isPending && <span className="text-base text-muted-foreground">Ładowanie…</span>}
          {others.map((person) => {
            const state = invited.get(person.id)
            return (
              <div
                key={person.id}
                className={cn(
                  'flex flex-col gap-3 rounded-lg border bg-card px-4 py-3',
                  state && 'border-primary/60',
                )}
              >
                <label className="flex cursor-pointer items-center gap-4">
                  <Checkbox
                    checked={state !== undefined}
                    onCheckedChange={() => toggle(person.id)}
                    aria-label={person.username}
                  />
                  <UserAvatar username={person.username} src={person.avatar_url} size="md" />
                  <span className="text-metal text-lg font-medium">{person.username}</span>
                </label>
                {state && kind.money === 'wager' && (
                  <TermsFields
                    kind={kind}
                    sides={[]}
                    state={state}
                    onChange={(next) => setTerms(person.id, next)}
                    idPrefix={`invite-${person.id}`}
                  />
                )}
              </div>
            )
          })}
          {create.isError && <FieldError message={errors.fields.opponents} />}
        </fieldset>

        <details className="rounded-lg border bg-card px-4 py-3">
          <summary className="cursor-pointer text-base font-medium">Dodatkowe informacje</summary>
          <Textarea
            className="mt-3"
            aria-label="Dodatkowe informacje"
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
          />
        </details>

        {(localError ?? errors.general) && (create.isError || localError) && (
          <span role="alert" className="text-base text-destructive">
            {localError ?? errors.general}
          </span>
        )}
        {create.isError && !errors.general && !localError && errors.fields.kind && (
          <FieldError message={errors.fields.kind} />
        )}

        <Button type="submit" disabled={create.isPending}>
          {create.isPending ? 'Wysyłanie…' : 'Zaproponuj'}
        </Button>
      </form>
    </>
  )
}
