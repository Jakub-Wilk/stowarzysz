import { ImagePlus } from 'lucide-react'
import { useRef, useState, type ChangeEvent, type SubmitEvent } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { readPreview, validateImage } from '@/features/admin/avatar'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { usePeople } from '@/features/voting/hooks'
import { ScoreResultDisplay } from '@/features/voting/kinds/score'
import { toneClasses } from '@/features/voting/tone'
import { formErrors } from '@/lib/api-errors'
import type { PollDetail } from '@/lib/api-types'
import { cn } from '@/lib/utils'

import type { CreateFormProps } from './types'

const readString = (value: unknown): string => (typeof value === 'string' ? value : '')

/** Who the vote is about: a pick-one list of everyone but the caller. */
function TargetPicker({
  people,
  value,
  onChange,
}: Pick<CreateFormProps, 'people'> & { value: number | null; onChange: (id: number) => void }) {
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="mb-2 text-base font-medium">Którego posła dotyczy głosowanie?</legend>
      {people.map((person) => (
        <label
          key={person.id}
          className={cn(
            'flex cursor-pointer items-center gap-4 rounded-lg border bg-card px-4 py-3 transition-colors hover:bg-accent',
            value === person.id && 'border-primary',
          )}
        >
          <input
            type="radio"
            name="target"
            className="size-4 accent-primary"
            checked={value === person.id}
            onChange={() => onChange(person.id)}
            aria-label={person.username}
          />
          <UserAvatar username={person.username} src={person.avatar_url} size="md" />
          <span className="text-metal text-lg font-medium">{person.username}</span>
        </label>
      ))}
    </fieldset>
  )
}

function CreateFormShell({
  people,
  error,
  pending,
  canSubmit,
  onSubmit,
  target,
  onTarget,
  children,
}: Pick<CreateFormProps, 'people' | 'error' | 'pending'> & {
  canSubmit: boolean
  onSubmit: () => void
  target: number | null
  onTarget: (id: number) => void
  children: React.ReactNode
}) {
  const errors = error ? formErrors(error) : null
  const message = errors && (errors.general ?? Object.values(errors.fields).join(' '))
  return (
    <form
      onSubmit={(e: SubmitEvent<HTMLFormElement>) => {
        e.preventDefault()
        if (canSubmit && !pending) onSubmit()
      }}
      className="flex flex-col gap-6"
    >
      <TargetPicker people={people} value={target} onChange={onTarget} />
      {target !== null && children}
      <p className="text-base text-muted-foreground">
        Głosują wszyscy posłowie. Zmiana wejdzie w życie, jeśli średnia ocen wyniesie co najmniej 1,
        a nikt nie użyje weta. Głosowanie kończy się po oddaniu wszystkich głosów albo po 72
        godzinach.
      </p>
      {message && (
        <span role="alert" className="text-base text-destructive">
          {message}
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

export function NicknameCreateForm({ people, pending, error, onSubmit }: CreateFormProps) {
  const [target, setTarget] = useState<number | null>(null)
  const [name, setName] = useState('')
  const fieldError = error ? formErrors(error).fields.new_username : undefined
  return (
    <CreateFormShell
      people={people}
      pending={pending}
      error={fieldError ? null : error}
      canSubmit={target !== null && name.trim() !== ''}
      target={target}
      onTarget={setTarget}
      onSubmit={() =>
        target !== null &&
        onSubmit({ kind: 'nickname', config: { target_user_id: target, new_username: name } })
      }
    >
      <div className="flex flex-col gap-2">
        <Label htmlFor="new-username">Nowy nick</Label>
        <Input
          id="new-username"
          value={name}
          maxLength={150}
          aria-invalid={fieldError !== undefined}
          onChange={(e) => setName(e.target.value)}
        />
        {fieldError && (
          <span role="alert" className="text-base text-destructive">
            {fieldError}
          </span>
        )}
      </div>
    </CreateFormShell>
  )
}

export function AvatarCreateForm({ people, pending, error, onSubmit }: CreateFormProps) {
  const input = useRef<HTMLInputElement>(null)
  const [target, setTarget] = useState<number | null>(null)
  const [picture, setPicture] = useState<{ file: File; preview: string } | null>(null)
  const [remove, setRemove] = useState(false)
  const [clientError, setClientError] = useState<string | null>(null)
  const person = people.find((p) => p.id === target)

  const onFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const chosen = e.target.files?.[0]
    if (!chosen) return
    const problem = validateImage(chosen)
    setClientError(problem)
    if (problem) return
    // keep our own copy: the input's File may stop being readable once the input is reset
    const file = new File([await chosen.arrayBuffer()], chosen.name, { type: chosen.type })
    e.target.value = ''
    setRemove(false)
    setPicture({ file, preview: await readPreview(file) })
  }

  return (
    <CreateFormShell
      people={people}
      pending={pending}
      error={clientError ? null : error}
      canSubmit={target !== null && (remove || picture !== null)}
      target={target}
      onTarget={setTarget}
      onSubmit={() =>
        target !== null &&
        onSubmit({
          kind: 'avatar',
          config: { target_user_id: target, remove },
          image: remove ? undefined : picture?.file,
        })
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-5">
          <UserAvatar
            username={person?.username ?? ''}
            src={remove ? null : (picture?.preview ?? null)}
            size="xl"
          />
          <div className="flex flex-col items-start gap-2">
            <input
              ref={input}
              type="file"
              accept="image/*"
              className="hidden"
              aria-label="Plik zdjęcia profilowego"
              onChange={onFile}
            />
            <Button type="button" variant="outline" onClick={() => input.current?.click()}>
              <ImagePlus /> {picture ? 'Wybierz inne zdjęcie' : 'Wybierz zdjęcie'}
            </Button>
            {person?.avatar_url && (
              <Button
                type="button"
                variant={remove ? 'default' : 'ghost'}
                onClick={() => {
                  setRemove(!remove)
                  setPicture(null)
                }}
              >
                {remove ? 'Zdjęcie zostanie usunięte' : 'Zaproponuj usunięcie zdjęcia'}
              </Button>
            )}
          </div>
        </div>
        {clientError && (
          <span role="alert" className="text-base text-destructive">
            {clientError}
          </span>
        )}
      </div>
    </CreateFormShell>
  )
}

/** The proposed change, shown while voting and in the result. */
export function NicknameProposal({ poll }: { poll: PollDetail }) {
  return (
    <div className="flex flex-wrap items-center justify-center gap-3 rounded-2xl border bg-card p-5 text-2xl font-semibold">
      <span className="text-muted-foreground line-through">
        {readString(poll.config.target_username)}
      </span>
      <span aria-hidden>→</span>
      <span className="text-metal">{readString(poll.config.new_username)}</span>
    </div>
  )
}

export function AvatarProposal({ poll }: { poll: PollDetail }) {
  const { data: people } = usePeople()
  const targetId = poll.config.target_user_id
  const username = readString(poll.config.target_username)
  const live = people?.find((p) => p.id === targetId)?.avatar_url ?? null
  const removing = poll.config.remove === true
  const applied = poll.result?.applied === true
  // Once applied, the live picture is the new one: use the stored copies instead. Votes closed
  // before copies were kept have neither, so fall back to what we can know.
  const legacy = applied && poll.proposed_avatar_url === null && !removing
  const current = applied ? (legacy ? null : poll.previous_avatar_url) : live
  const proposed = poll.proposed_avatar_url ?? (legacy ? live : null)
  return (
    <div className="flex items-center justify-center gap-5 rounded-2xl border bg-card p-5">
      <UserAvatar username={username} src={current} size="xl" />
      <span aria-hidden className="text-2xl">
        →
      </span>
      {removing ? (
        <span className="text-lg font-semibold">usunięcie zdjęcia</span>
      ) : (
        <UserAvatar username={username} src={proposed} size="xl" />
      )}
    </div>
  )
}

/** Whether the winning change actually happened (it can fail, e.g. the nick was taken). */
function Outcome({ poll }: { poll: PollDetail }) {
  const result = poll.result
  const approved = result?.approved === true
  const applied = result?.applied === true
  const reason = readString(result?.apply_error)
  const text = applied
    ? 'Zmiana zastosowana'
    : approved
      ? `Przyjęto, ale nie udało się zastosować zmiany${reason ? `: ${reason}` : '.'}`
      : 'Nie przyjęto'
  const tone = toneClasses(applied ? 'positive' : approved ? 'neutral' : 'negative')
  return (
    <div
      role="status"
      className={cn('rounded-2xl border p-4 text-center text-xl font-bold', tone.panel)}
    >
      {text}
    </div>
  )
}

export function ProfileResultDisplay({ poll }: { poll: PollDetail }) {
  return (
    <div className="flex flex-col gap-4">
      <Outcome poll={poll} />
      <ScoreResultDisplay poll={poll} />
    </div>
  )
}
