import { Plus, X } from 'lucide-react'
import { useEffect, useState, type SubmitEvent } from 'react'
import { Link, useNavigate } from 'react-router'

import { Button, buttonVariants } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { ParticipantsField } from '@/features/voting/ParticipantsField'
import { ReasonBadge } from '@/features/voting/ReasonBadge'
import { useTieBreak } from '@/features/voting/hooks'
import {
  MAX_LABEL_LENGTH,
  MAX_OPTIONS,
  MAX_OPTIONS_OPEN,
  MIN_OPTIONS,
  isOpenPool,
  readMaxChoices,
  readOptions,
  readPicks,
  readResult,
  readTieBreakOf,
  type OptionsResult,
} from '@/features/voting/kinds/optionsData'
import { toneClasses } from '@/features/voting/tone'
import { formErrors } from '@/lib/api-errors'
import type { PollDetail, PollResult } from '@/lib/api-types'
import { plural, VOTE_FORMS, type PluralForms } from '@/lib/plural'
import { cn } from '@/lib/utils'

import type { BallotInputProps, CreateFormProps } from './types'

const FIRST_PLACE_FORMS: PluralForms = {
  one: 'pierwsze miejsce',
  few: 'pierwsze miejsca',
  many: 'pierwszych miejsc',
}

const ADD_HINT = 'Możesz dodać jedną nową opcję przy każdym głosie.'

// --- ballots (single and multi choice) ----------------------------------------------------------

function ChoiceCard({
  selected,
  disabled,
  children,
}: {
  selected: boolean
  disabled?: boolean
  children: React.ReactNode
}) {
  return (
    <label
      className={cn(
        'flex cursor-pointer items-center gap-4 rounded-lg border bg-card px-4 py-3 transition-colors hover:bg-accent',
        selected && 'border-primary',
        disabled && 'cursor-not-allowed opacity-50 hover:bg-card',
      )}
    >
      {children}
    </label>
  )
}

/** The text box for the one option a voter may add with their vote (open pools only). */
function NewOptionInput({
  value,
  onChange,
  disabled,
  placeholder,
  ...rest
}: {
  value: string
  onChange: (value: string) => void
  disabled?: boolean
  placeholder: string
  onFocus?: () => void
}) {
  return (
    <Input
      value={value}
      maxLength={MAX_LABEL_LENGTH}
      disabled={disabled}
      placeholder={placeholder}
      aria-label={placeholder}
      className="h-10 flex-1"
      onChange={(e) => onChange(e.target.value)}
      {...rest}
    />
  )
}

export function SingleBallotInput({ poll, value, pending, onSubmit }: BallotInputProps) {
  const options = readOptions(poll.config)
  const canAdd = isOpenPool(poll.config) && options.length < MAX_OPTIONS_OPEN
  const saved = readPicks(value)[0] ?? null
  const [picked, setPicked] = useState<string | null>(saved)
  const [custom, setCustom] = useState('')
  const [customPicked, setCustomPicked] = useState(false)
  const label = custom.trim()
  const ready = customPicked ? label !== '' : picked !== null && picked !== saved

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!ready || pending) return
    onSubmit(customPicked ? { new_option: label } : { choice: picked })
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <fieldset className="flex flex-col gap-2">
        <legend className="sr-only">Opcje</legend>
        {options.map((option) => (
          <ChoiceCard key={option.id} selected={!customPicked && picked === option.id}>
            <input
              type="radio"
              name="choice"
              className="size-4 accent-primary"
              checked={!customPicked && picked === option.id}
              onChange={() => {
                setPicked(option.id)
                setCustomPicked(false)
              }}
            />
            <span className="text-metal text-lg font-medium break-words">{option.label}</span>
          </ChoiceCard>
        ))}
        {canAdd && (
          <ChoiceCard selected={customPicked}>
            <input
              type="radio"
              name="choice"
              className="size-4 accent-primary"
              aria-label="Inna opcja"
              checked={customPicked}
              onChange={() => setCustomPicked(true)}
            />
            <NewOptionInput
              value={custom}
              placeholder="Inna opcja"
              onChange={setCustom}
              onFocus={() => setCustomPicked(true)}
            />
          </ChoiceCard>
        )}
      </fieldset>
      {canAdd && <p className="text-sm text-muted-foreground">{ADD_HINT}</p>}
      <Button type="submit" disabled={!ready || pending}>
        {saved === null ? 'Oddaj głos' : 'Zmień głos'}
      </Button>
    </form>
  )
}

const sameSet = (a: ReadonlySet<string>, b: string[]) =>
  a.size === b.length && b.every((id) => a.has(id))

export function MultiBallotInput({ poll, value, pending, onSubmit }: BallotInputProps) {
  const options = readOptions(poll.config)
  const canAdd = isOpenPool(poll.config) && options.length < MAX_OPTIONS_OPEN
  const limit = readMaxChoices(poll.config)
  const saved = readPicks(value)
  const [picked, setPicked] = useState<ReadonlySet<string>>(new Set(saved))
  const [custom, setCustom] = useState('')
  const label = custom.trim()
  const count = picked.size + (label ? 1 : 0)
  const full = limit !== null && count >= limit
  const ready =
    count >= 1 && (limit === null || count <= limit) && (label !== '' || !sameSet(picked, saved))

  const toggle = (id: string) => {
    const next = new Set(picked)
    if (!next.delete(id)) next.add(id)
    setPicked(next)
  }

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!ready || pending) return
    onSubmit({ choices: [...picked], ...(label ? { new_option: label } : {}) })
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <fieldset className="flex flex-col gap-2">
        <legend className="sr-only">Opcje</legend>
        {options.map((option) => {
          const checked = picked.has(option.id)
          return (
            <ChoiceCard key={option.id} selected={checked} disabled={full && !checked}>
              <Checkbox
                checked={checked}
                disabled={full && !checked}
                onCheckedChange={() => toggle(option.id)}
                aria-label={option.label}
              />
              <span className="text-metal text-lg font-medium break-words">{option.label}</span>
            </ChoiceCard>
          )
        })}
        {canAdd && (
          <div className="flex items-center gap-4 rounded-lg border bg-card px-4 py-3">
            <Plus className="size-6 shrink-0 text-muted-foreground" aria-hidden />
            <NewOptionInput
              value={custom}
              placeholder="Dodaj opcję"
              onChange={setCustom}
              disabled={full && label === ''}
            />
          </div>
        )}
      </fieldset>
      <p className="text-sm text-muted-foreground">
        {limit !== null && `Wybierz do ${limit}. `}
        {canAdd && ADD_HINT}
      </p>
      <Button type="submit" disabled={!ready || pending}>
        {saved.length === 0 ? 'Oddaj głos' : 'Zmień głos'}
      </Button>
    </form>
  )
}

// --- result -------------------------------------------------------------------------------------

/** A bar that fills in after mount; transform only. */
function Bar({ ratio, highlight }: { ratio: number; highlight: boolean }) {
  const [shown, setShown] = useState(0)
  useEffect(() => {
    const frame = requestAnimationFrame(() => setShown(ratio))
    return () => cancelAnimationFrame(frame)
  }, [ratio])
  return (
    <div className="h-2 overflow-hidden rounded-full bg-muted">
      <div
        className={cn(
          'h-full origin-left transition-transform duration-700 ease-(--ease-out-soft)',
          highlight ? 'bg-positive' : 'bg-primary/60',
        )}
        style={{ transform: `scaleX(${shown})` }}
      />
    </div>
  )
}

function headline(result: OptionsResult | null): string {
  if (!result || result.winners.length === 0) return 'Nie oddano głosów'
  const labels = result.winners.map((id) => result.options.find((o) => o.id === id)?.label ?? id)
  return labels.length === 1 ? `Wygrywa: ${labels[0]}` : `Remis: ${labels.join(', ')}`
}

function Summary({ poll, result }: { poll: PollDetail; result: OptionsResult | null }) {
  const tone = toneClasses(poll.result?.tone)
  const decided = result !== null && result.winners.length === 1
  return (
    <div className={cn('flex flex-col items-center gap-2 rounded-2xl border p-6', tone.panel)}>
      <div className={cn('text-center text-3xl font-black break-words', decided && tone.text)}>
        {headline(result)}
      </div>
      {result?.condorcet_winner && (
        <div className="text-base text-muted-foreground">Pokonuje każdą inną opcję</div>
      )}
      {result && (
        <div className="text-base text-muted-foreground">
          Zagłosowało {result.votes_cast} z {poll.participant_count} posłów
          {result.abstain_count > 0 && ` · wstrzymało się: ${result.abstain_count}`}
        </div>
      )}
    </div>
  )
}

function OptionBars({ result, ranked }: { result: OptionsResult; ranked: boolean }) {
  const others = result.options.length - 1
  return (
    <ul className="flex flex-col gap-3">
      {result.options.map((option) => {
        const winner = result.winners.includes(option.id)
        const ratio = ranked
          ? others > 0
            ? (option.wins ?? 0) / others
            : 0
          : result.votes_cast > 0
            ? option.count / result.votes_cast
            : 0
        return (
          <li
            key={option.id}
            className={cn(
              'flex flex-col gap-2 rounded-lg border bg-card px-4 py-3',
              winner && 'border-positive/60',
            )}
          >
            <div className="flex items-baseline justify-between gap-3">
              <span className="text-metal text-lg font-medium break-words">{option.label}</span>
              <span className="shrink-0 text-base font-bold tabular-nums">
                {ranked
                  ? `pokonuje ${option.wins ?? 0} z ${others}`
                  : `${option.count} ${plural(option.count, VOTE_FORMS)}`}
              </span>
            </div>
            <Bar ratio={ratio} highlight={winner} />
            {ranked && (
              <span className="text-sm text-muted-foreground">
                {option.count} {plural(option.count, FIRST_PLACE_FORMS)}
              </span>
            )}
          </li>
        )
      })}
    </ul>
  )
}

/** Head-to-head counts behind a ranked result, so the outcome can be checked by hand. */
function HowCounted({ result }: { result: OptionsResult }) {
  const { pairwise, options } = result
  if (!pairwise) return null
  const pairs = options.flatMap((a, i) => options.slice(i + 1).map((b) => [a, b] as const))
  return (
    <details className="rounded-lg border bg-card px-4 py-3">
      <summary className="cursor-pointer text-base font-medium">Jak liczono</summary>
      <p className="mt-2 text-sm text-muted-foreground">
        Metoda Schulzego: każda para opcji porównywana jest głos po głosie, a wygrywa ta, której
        zwycięstwa (także pośrednie) są najmocniejsze.
      </p>
      <ul className="mt-3 flex flex-col gap-1 text-base">
        {pairs.map(([a, b]) => {
          const forA = pairwise[a.id]?.[b.id] ?? 0
          const forB = pairwise[b.id]?.[a.id] ?? 0
          return (
            <li key={`${a.id}-${b.id}`}>
              {forA === forB
                ? `${a.label} i ${b.label} remisują ${forA}:${forB}`
                : forA > forB
                  ? `${a.label} wygrywa z ${b.label} ${forA}:${forB}`
                  : `${b.label} wygrywa z ${a.label} ${forB}:${forA}`}
            </li>
          )
        })}
      </ul>
    </details>
  )
}

/** Only the creator can call it; once it exists, everyone gets a link to it. */
function TieBreak({ poll }: { poll: PollDetail }) {
  const navigate = useNavigate()
  const tieBreak = useTieBreak(poll.id)
  if (poll.tie_break_poll !== null) {
    return (
      <Link
        to={`/voting/${poll.tie_break_poll}`}
        className={cn(buttonVariants({ variant: 'outline' }), 'w-full')}
      >
        Zobacz dogrywkę
      </Link>
    )
  }
  if (!poll.can_tie_break) return null
  return (
    <section className="flex flex-col gap-2 rounded-lg border bg-card p-4">
      <p className="text-base text-muted-foreground">
        Jako wnioskodawca możesz rozpisać dogrywkę: nowe głosowanie tych samych posłów tylko nad
        opcjami, które zremisowały.
      </p>
      <Button
        disabled={tieBreak.isPending}
        onClick={() =>
          tieBreak.mutate(undefined, { onSuccess: (next) => navigate(`/voting/${next.id}`) })
        }
      >
        Rozstrzygnij remis
      </Button>
      {tieBreak.isError && (
        <span role="alert" className="text-base text-destructive">
          {formErrors(tieBreak.error).general ??
            Object.values(formErrors(tieBreak.error).fields).join(' ')}
        </span>
      )}
    </section>
  )
}

function OptionsResultDisplay({ poll, ranked }: { poll: PollDetail; ranked: boolean }) {
  const result = readResult(poll.result)
  const labels = new Map(readOptions(poll.config).map((o) => [o.id, o.label]))
  const name = (id: string) => labels.get(id) ?? id

  return (
    <div className="flex flex-col gap-4">
      <Summary poll={poll} result={result} />
      {result && result.votes_cast > 0 && <OptionBars result={result} ranked={ranked} />}
      {ranked && result && result.votes_cast > 0 && <HowCounted result={result} />}
      <TieBreak poll={poll} />

      <ul className="flex flex-col gap-2">
        {poll.participants.map(({ user, has_voted, ballot, abstained, reason, reaction }) => {
          const picks = readPicks(ballot)
          return (
            <li key={user.id} className="flex flex-col gap-2 rounded-lg border bg-card px-4 py-3">
              <div className="flex items-center gap-3">
                <UserAvatar username={user.username} src={user.avatar_url} size="md" />
                <span className="text-metal flex-1 truncate text-lg font-medium">
                  {user.username}
                </span>
                <span
                  className="w-8 text-center text-xl"
                  aria-label={reaction ? `Reakcja ${reaction}` : undefined}
                >
                  {reaction}
                </span>
                {abstained && (
                  <span className="flex w-12 justify-center">
                    <ReasonBadge type="abstain" reason={reason} />
                  </span>
                )}
                {!abstained && !(has_voted && picks.length > 0) && (
                  <span className="text-base text-muted-foreground">brak głosu</span>
                )}
              </div>
              {!abstained && picks.length > 0 && (
                <ol
                  className={cn(
                    'flex flex-wrap gap-2 text-base',
                    ranked && 'list-inside list-decimal flex-col gap-1',
                  )}
                >
                  {picks.map((id) => (
                    <li
                      key={id}
                      className={cn(
                        !ranked && 'list-none rounded-full bg-muted px-3 py-0.5 font-medium',
                      )}
                    >
                      {name(id)}
                    </li>
                  ))}
                </ol>
              )}
            </li>
          )
        })}
      </ul>
    </div>
  )
}

export const SingleResultDisplay = ({ poll }: { poll: PollDetail }) => (
  <OptionsResultDisplay poll={poll} ranked={false} />
)
export const RankedResultDisplay = ({ poll }: { poll: PollDetail }) => (
  <OptionsResultDisplay poll={poll} ranked />
)

export function OptionsResultChip({ result }: { result: PollResult }) {
  const parsed = readResult(result)
  const tone = toneClasses(result.tone)
  const text =
    !parsed || parsed.winners.length === 0
      ? '—'
      : parsed.winners.length > 1
        ? 'Remis'
        : (parsed.options.find((o) => o.id === parsed.winners[0])?.label ?? '—')
  return (
    <span
      className={cn('max-w-40 truncate rounded-full px-2.5 py-0.5 text-sm font-bold', tone.chip)}
    >
      {text}
    </span>
  )
}

/** Shown above the ballot and the result: the link back from a tie-break, the open-pool note. */
export function OptionsProposal({ poll }: { poll: PollDetail }) {
  const original = readTieBreakOf(poll.config)
  const open = poll.status === 'open' && isOpenPool(poll.config)
  if (original === null && !open) return null
  return (
    <div className="flex flex-col gap-1 rounded-lg border bg-card px-4 py-3 text-base">
      {original !== null && (
        <span>
          Dogrywka do{' '}
          <Link to={`/voting/${original}`} className="underline underline-offset-4">
            wcześniejszego głosowania
          </Link>
          . Wybierz spośród opcji, które zremisowały.
        </span>
      )}
      {open && (
        <span className="text-muted-foreground">
          Poseł może dodać nową opcję, głosując na nią. Dodane opcje widać od razu.
        </span>
      )}
    </div>
  )
}

// --- create form --------------------------------------------------------------------------------

type OptionsKindKey = 'single_choice' | 'multi_choice' | 'ranked_choice'

function OptionsEditor({
  labels,
  onChange,
  error,
}: {
  labels: string[]
  onChange: (labels: string[]) => void
  error?: string
}) {
  const set = (index: number, value: string) =>
    onChange(labels.map((label, i) => (i === index ? value : label)))
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-2 text-base font-medium">Opcje</legend>
      {labels.map((label, index) => (
        <div key={index} className="flex items-center gap-2">
          <Input
            value={label}
            maxLength={MAX_LABEL_LENGTH}
            aria-label={`Opcja ${index + 1}`}
            placeholder={`Opcja ${index + 1}`}
            onChange={(e) => set(index, e.target.value)}
          />
          {labels.length > MIN_OPTIONS && (
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={`Usuń opcję ${index + 1}`}
              onClick={() => onChange(labels.filter((_, i) => i !== index))}
            >
              <X />
            </Button>
          )}
        </div>
      ))}
      {labels.length < MAX_OPTIONS && (
        <div>
          <Button type="button" variant="outline" onClick={() => onChange([...labels, ''])}>
            <Plus /> Dodaj opcję
          </Button>
        </div>
      )}
      {error && (
        <span role="alert" className="text-base text-destructive">
          {error}
        </span>
      )}
    </fieldset>
  )
}

function OptionsCreateForm({
  kind,
  people,
  pending,
  error,
  onSubmit,
}: CreateFormProps & { kind: OptionsKindKey }) {
  const [title, setTitle] = useState('')
  const [labels, setLabels] = useState(['', ''])
  const [maxChoices, setMaxChoices] = useState('')
  const [openPool, setOpenPool] = useState(false)
  const [chosen, setChosen] = useState<ReadonlySet<number>>(new Set())
  const errors = error ? formErrors(error) : null
  const filled = labels.map((l) => l.trim()).filter(Boolean)
  const canSubmit = title.trim() !== '' && filled.length >= MIN_OPTIONS

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!canSubmit || pending) return
    const limit = Number(maxChoices)
    onSubmit({
      title: title.trim(),
      kind,
      config: {
        options: filled,
        ...(kind !== 'ranked_choice' && openPool ? { open_options: true } : {}),
        ...(kind === 'multi_choice' && maxChoices !== '' && Number.isInteger(limit)
          ? { max_choices: limit }
          : {}),
      },
      participant_ids: [...chosen],
    })
  }

  return (
    <form data-slot="entry-form" onSubmit={submit} className="flex flex-col gap-6">
      <div className="flex flex-col gap-2">
        <Label htmlFor="poll-title">Pytanie</Label>
        <Input
          id="poll-title"
          value={title}
          maxLength={200}
          aria-invalid={errors?.fields.title !== undefined}
          onChange={(e) => setTitle(e.target.value)}
          autoFocus
        />
        {errors?.fields.title && (
          <span role="alert" className="text-base text-destructive">
            {errors.fields.title}
          </span>
        )}
      </div>

      <OptionsEditor labels={labels} onChange={setLabels} error={errors?.fields.options} />

      {kind === 'multi_choice' && (
        <div className="flex flex-col gap-2">
          <Label htmlFor="max-choices">Limit wyborów (opcjonalnie)</Label>
          <Input
            id="max-choices"
            type="number"
            inputMode="numeric"
            min={1}
            max={Math.max(1, filled.length - 1)}
            value={maxChoices}
            aria-invalid={errors?.fields.max_choices !== undefined}
            onChange={(e) => setMaxChoices(e.target.value)}
          />
          {errors?.fields.max_choices && (
            <span role="alert" className="text-base text-destructive">
              {errors.fields.max_choices}
            </span>
          )}
        </div>
      )}

      {kind !== 'ranked_choice' && (
        <div className="flex items-center justify-between gap-4">
          <div className="flex flex-col gap-1">
            <span id="open-pool-label" className="text-metal text-base font-medium">
              Pozwól posłom dodawać opcje
            </span>
            <span className="text-sm text-muted-foreground">
              Przy oddawaniu głosu poseł może dodać jedną nową opcję (łącznie do {MAX_OPTIONS_OPEN}
              ). Ci, którzy już zagłosowali, dostaną powiadomienie.
            </span>
          </div>
          <Switch
            aria-labelledby="open-pool-label"
            checked={openPool}
            onCheckedChange={setOpenPool}
          />
        </div>
      )}

      <ParticipantsField
        people={people}
        chosen={chosen}
        onChange={setChosen}
        error={errors?.fields.participant_ids}
      />

      {errors?.general && (
        <span role="alert" className="text-base text-destructive">
          {errors.general}
        </span>
      )}
      <div>
        <Button type="submit" disabled={!canSubmit || pending}>
          Zarządź głosowanie
        </Button>
      </div>
    </form>
  )
}

export const SingleCreateForm = (props: CreateFormProps) => (
  <OptionsCreateForm kind="single_choice" {...props} />
)
export const MultiCreateForm = (props: CreateFormProps) => (
  <OptionsCreateForm kind="multi_choice" {...props} />
)
export const RankedCreateForm = (props: CreateFormProps) => (
  <OptionsCreateForm kind="ranked_choice" {...props} />
)
