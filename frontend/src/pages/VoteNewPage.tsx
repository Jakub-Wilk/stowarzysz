import { useState, type SubmitEvent } from 'react'
import { useNavigate } from 'react-router'

import { BackLink } from '@/components/layout/BackLink'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { useMe } from '@/features/auth/hooks'
import { useCreatePoll, usePeople } from '@/features/voting/hooks'
import { getKindUI } from '@/features/voting/kinds'
import { ParticipantsField } from '@/features/voting/ParticipantsField'
import { formErrors } from '@/lib/api-errors'

const KIND_CHOICES = [
  { key: 'score', label: 'Na minus pięć' },
  { key: 'nickname', label: 'Zmiana nicku' },
  { key: 'avatar', label: 'Zmiana zdjęcia' },
  { key: 'single_choice', label: 'Wybierz 1' },
  { key: 'multi_choice', label: 'Wybierz wiele' },
  { key: 'ranked_choice', label: 'Ranking' },
] as const

/**
 * Call a vote: pick the kind, then either a title and who takes part (the caller always does) or,
 * for a nickname / profile-picture vote, whose profile it is about and the proposed change.
 */
export function VoteNewPage() {
  const navigate = useNavigate()
  const { data: me } = useMe()
  const people = usePeople()
  const create = useCreatePoll()
  const [kindKey, setKindKey] = useState<string>('score')
  const [title, setTitle] = useState('')
  const [chosen, setChosen] = useState<ReadonlySet<number>>(new Set())

  const others = (people.data ?? []).filter((p) => p.id !== me?.id)

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!title.trim() || create.isPending) return
    create.mutate(
      { title: title.trim(), kind: 'score', config: {}, participant_ids: [...chosen] },
      { onSuccess },
    )
  }

  const errors = formErrors(create.error)
  const CreateForm = getKindUI(kindKey)?.CreateForm
  const onSuccess = (poll: { id: number }) => navigate(`/voting/${poll.id}`, { replace: true })

  return (
    <>
      <BackLink to="/voting">Sejmik</BackLink>
      <h2 className="mb-6 text-2xl font-semibold">Nowe głosowanie</h2>
      <div
        className="mb-6 grid max-w-md grid-cols-2 gap-2 sm:grid-cols-3"
        role="group"
        aria-label="Rodzaj głosowania"
      >
        {KIND_CHOICES.map(({ key, label }) => (
          <Button
            key={key}
            type="button"
            variant={kindKey === key ? 'default' : 'outline'}
            aria-pressed={kindKey === key}
            onClick={() => {
              setKindKey(key)
              create.reset()
            }}
          >
            {label}
          </Button>
        ))}
      </div>
      {CreateForm ? (
        <div className="max-w-md">
          <CreateForm
            key={kindKey}
            people={others}
            pending={create.isPending}
            error={create.isError ? create.error : null}
            onSubmit={(payload) => create.mutate(payload, { onSuccess })}
          />
        </div>
      ) : (
        <form data-slot="entry-form" onSubmit={submit} className="flex max-w-md flex-col gap-6">
          <div className="flex flex-col gap-2">
            <Label htmlFor="title">Przedmiot głosowania</Label>
            <Input
              id="title"
              value={title}
              maxLength={200}
              aria-invalid={errors.fields.title !== undefined}
              onChange={(e) => setTitle(e.target.value)}
              autoFocus
            />
            {errors.fields.title && create.isError && (
              <span role="alert" className="text-base text-destructive">
                {errors.fields.title}
              </span>
            )}
          </div>

          <ParticipantsField
            people={others}
            chosen={chosen}
            onChange={setChosen}
            loading={people.isPending}
            error={create.isError ? errors.fields.participant_ids : undefined}
          />

          {create.isError && errors.general && (
            <span role="alert" className="text-base text-destructive">
              {errors.general}
            </span>
          )}
          <div>
            <Button type="submit" disabled={!title.trim() || create.isPending}>
              Zarządź głosowanie
            </Button>
          </div>
        </form>
      )}
    </>
  )
}
