import { useState, type SubmitEvent } from 'react'

import { BackLink } from '@/components/layout/BackLink'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { DeadlineField, ModeField, TiersField } from '@/features/secretsanta/SantaFields'
import { formatAmounts, parseTiers, toLocalInput } from '@/features/secretsanta/format'
import { useEndSanta, useSanta, useStartSanta, useUpdateSanta } from '@/features/secretsanta/hooks'
import { ConfirmDialog } from '@/features/voting/ConfirmDialog'
import { usePeople } from '@/features/voting/hooks'
import { formErrors } from '@/lib/api-errors'
import type { SantaEvent, SantaMode } from '@/lib/api-types'
import { PARTICIPANT_FORMS, plural } from '@/lib/plural'

const INVALID_TIERS = 'Podaj dodatnie, całkowite kwoty.'

function StartForm() {
  const people = usePeople()
  const start = useStartSanta()
  const [chosen, setChosen] = useState<ReadonlySet<number>>(new Set())
  const [deadline, setDeadline] = useState('')
  const [tiers, setTiers] = useState<string[]>(['100', '30'])
  const [tiersError, setTiersError] = useState<string>()
  const [mode, setMode] = useState<SantaMode>('single')

  const everyone = people.data ?? []
  const allChosen = everyone.length > 0 && everyone.every((p) => chosen.has(p.id))
  const errors = formErrors(start.error)

  const toggle = (id: number) =>
    setChosen((current) => {
      const next = new Set(current)
      if (!next.delete(id)) next.add(id)
      return next
    })

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    const amounts = parseTiers(tiers)
    setTiersError(amounts ? undefined : INVALID_TIERS)
    if (!amounts || !deadline || start.isPending) return
    start.mutate({
      participant_ids: [...chosen],
      deadline: new Date(deadline).toISOString(),
      gift_tiers: amounts,
      mode,
    })
  }

  return (
    <form onSubmit={submit} className="flex max-w-md flex-col gap-6">
      <fieldset className="flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <legend className="text-base font-medium">Kto bierze udział?</legend>
          {everyone.length > 0 && (
            <Button
              type="button"
              variant="ghost"
              onClick={() => setChosen(allChosen ? new Set() : new Set(everyone.map((p) => p.id)))}
            >
              {allChosen ? 'Odznacz wszystkich' : 'Zaznacz wszystkich'}
            </Button>
          )}
        </div>
        {people.isPending && <span className="text-base text-muted-foreground">Ładowanie…</span>}
        {everyone.map((person) => (
          <label
            key={person.id}
            className="flex cursor-pointer items-center gap-4 rounded-lg border bg-card px-4 py-3 transition-colors hover:bg-accent"
          >
            <Checkbox
              checked={chosen.has(person.id)}
              onCheckedChange={() => toggle(person.id)}
              aria-label={person.username}
            />
            <UserAvatar username={person.username} src={person.avatar_url} size="md" />
            <span className="text-metal text-lg font-medium">{person.username}</span>
          </label>
        ))}
        {errors.fields.participant_ids && start.isError && (
          <span role="alert" className="text-sm text-destructive">
            {errors.fields.participant_ids}
          </span>
        )}
      </fieldset>

      <DeadlineField
        value={deadline}
        onChange={setDeadline}
        error={start.isError ? errors.fields.deadline : undefined}
      />
      <ModeField value={mode} onChange={setMode} />
      <TiersField
        value={tiers}
        onChange={setTiers}
        error={tiersError ?? (start.isError ? errors.fields.gift_tiers : undefined)}
      />

      {start.isError && errors.general && (
        <span role="alert" className="text-base text-destructive">
          {errors.general}
        </span>
      )}
      <div>
        <Button type="submit" disabled={chosen.size === 0 || !deadline || start.isPending}>
          Rozpocznij losowanie
        </Button>
      </div>
    </form>
  )
}

function ActiveForm({ event }: { event: SantaEvent }) {
  const update = useUpdateSanta()
  const end = useEndSanta()
  const [deadline, setDeadline] = useState(() => toLocalInput(event.deadline))
  const [tiers, setTiers] = useState(() => event.gift_tiers.map(String))
  const [tiersError, setTiersError] = useState<string>()
  const [confirmEnd, setConfirmEnd] = useState(false)
  const errors = formErrors(update.error)

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    const amounts = parseTiers(tiers)
    setTiersError(amounts ? undefined : INVALID_TIERS)
    if (!amounts || !deadline || update.isPending) return
    update.mutate({ deadline: new Date(deadline).toISOString(), gift_tiers: amounts })
  }

  return (
    <div className="flex max-w-md flex-col gap-8">
      <p className="text-base text-muted-foreground">
        Trwa losowanie: {event.participants.length}{' '}
        {plural(event.participants.length, PARTICIPANT_FORMS)}. Kwoty:{' '}
        {formatAmounts(event.gift_tiers)}.{' '}
        {event.mode === 'per_tier'
          ? 'Każda kwota ma innego podopiecznego, więc można zmienić tylko wartości kwot, nie ich liczbę. '
          : ''}
        Zmiana kwot nie zmienia losowania, ale uczestnicy dostaną powiadomienie.
      </p>
      <form onSubmit={submit} className="flex flex-col gap-6">
        <DeadlineField
          value={deadline}
          onChange={setDeadline}
          error={update.isError ? errors.fields.deadline : undefined}
        />
        <TiersField
          value={tiers}
          onChange={setTiers}
          error={tiersError ?? (update.isError ? errors.fields.gift_tiers : undefined)}
          fixedCount={event.mode === 'per_tier'}
        />
        {update.isError && errors.general && (
          <span role="alert" className="text-base text-destructive">
            {errors.general}
          </span>
        )}
        <div>
          <Button type="submit" disabled={!deadline || update.isPending}>
            Zapisz zmiany
          </Button>
        </div>
      </form>

      <section className="flex flex-col gap-2 border-t pt-6">
        <h3 className="text-lg font-semibold">Zakończ Secret Santa</h3>
        <p className="text-sm text-muted-foreground">
          Po zakończeniu wszystkie pary stają się publiczne i trafiają do historii.
        </p>
        <div>
          <Button variant="destructive" onClick={() => setConfirmEnd(true)}>
            Zakończ Secret Santa
          </Button>
        </div>
      </section>
      <ConfirmDialog
        open={confirmEnd}
        onOpenChange={setConfirmEnd}
        title="Zakończyć Secret Santa?"
        description="Wszystkie pary zostaną ujawnione i nie da się tego cofnąć."
        confirmLabel="Zakończ"
        pending={end.isPending}
        onConfirm={() => end.mutate(undefined, { onSuccess: () => setConfirmEnd(false) })}
      />
    </div>
  )
}

/** `/manage/secret-santa`: start a round, or adjust / end the running one. */
export function SantaManagePage() {
  const { data, isPending, error, refetch } = useSanta()

  return (
    <>
      <BackLink to="/manage">Zarządzanie</BackLink>
      <h2 className="mb-6 text-xl font-semibold">Secret Santa</h2>
      {isPending && <span className="text-sm text-muted-foreground">Ładowanie…</span>}
      {error && (
        <div className="flex flex-col items-start gap-2">
          <span role="alert" className="text-sm text-destructive">
            Nie udało się wczytać Secret Santa.
          </span>
          <Button variant="outline" onClick={() => refetch()}>
            Spróbuj ponownie
          </Button>
        </div>
      )}
      {data && (data.event ? <ActiveForm key={data.event.id} event={data.event} /> : <StartForm />)}
    </>
  )
}
