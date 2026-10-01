import { useState, type SubmitEvent } from 'react'
import { useNavigate } from 'react-router'

import { BackLink } from '@/components/layout/BackLink'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { useCreatePoll, usePeople } from '@/features/voting/hooks'
import { formErrors } from '@/lib/api-errors'

/** Call a vote: a title and who takes part (the caller always does). */
export function VoteNewPage() {
  const navigate = useNavigate()
  const { data: me } = useMe()
  const people = usePeople()
  const create = useCreatePoll()
  const [title, setTitle] = useState('')
  const [chosen, setChosen] = useState<ReadonlySet<number>>(new Set())

  const others = (people.data ?? []).filter((p) => p.id !== me?.id)
  const allChosen = others.length > 0 && others.every((p) => chosen.has(p.id))

  const toggle = (id: number) =>
    setChosen((current) => {
      const next = new Set(current)
      if (!next.delete(id)) next.add(id)
      return next
    })

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!title.trim() || create.isPending) return
    create.mutate(
      { title: title.trim(), kind: 'score', config: {}, participant_ids: [...chosen] },
      { onSuccess: (poll) => navigate(`/voting/${poll.id}`, { replace: true }) },
    )
  }

  const errors = formErrors(create.error)

  return (
    <>
      <BackLink to="/voting">Voting</BackLink>
      <h2 className="mb-6 text-2xl font-semibold">New vote</h2>
      <form onSubmit={submit} className="flex max-w-md flex-col gap-6">
        <div className="flex flex-col gap-2">
          <Label htmlFor="title">What are we voting on?</Label>
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

        <fieldset className="flex flex-col gap-2">
          <div className="flex items-center justify-between">
            <legend className="text-base font-medium">Who votes?</legend>
            {others.length > 0 && (
              <Button
                type="button"
                variant="ghost"
                onClick={() => setChosen(allChosen ? new Set() : new Set(others.map((p) => p.id)))}
              >
                {allChosen ? 'Select none' : 'Select all'}
              </Button>
            )}
          </div>
          {me && (
            <div className="flex items-center gap-4 rounded-lg border bg-card px-4 py-3 opacity-80">
              <Checkbox checked disabled aria-label="You" />
              <UserAvatar username={me.username} src={me.avatar_url} size="md" />
              <span className="text-metal text-lg font-medium">{me.username} (you)</span>
            </div>
          )}
          {people.isPending && <span className="text-base text-muted-foreground">Loading…</span>}
          {others.map((person) => (
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
          {errors.fields.participant_ids && create.isError && (
            <span role="alert" className="text-base text-destructive">
              {errors.fields.participant_ids}
            </span>
          )}
        </fieldset>

        {create.isError && errors.general && (
          <span role="alert" className="text-base text-destructive">
            {errors.general}
          </span>
        )}
        <div>
          <Button type="submit" disabled={!title.trim() || create.isPending}>
            Call vote
          </Button>
        </div>
      </form>
    </>
  )
}
