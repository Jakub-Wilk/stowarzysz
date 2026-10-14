import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import type { UserBrief } from '@/lib/api-types'

interface ParticipantsFieldProps {
  /** Everyone except the signed-in user, who always takes part. */
  people: UserBrief[]
  chosen: ReadonlySet<number>
  onChange: (chosen: ReadonlySet<number>) => void
  loading?: boolean
  error?: string
}

/** Who takes part in a vote: the caller (always) plus any of the other members. */
export function ParticipantsField({
  people,
  chosen,
  onChange,
  loading = false,
  error,
}: ParticipantsFieldProps) {
  const { data: me } = useMe()
  const allChosen = people.length > 0 && people.every((p) => chosen.has(p.id))

  const toggle = (id: number) => {
    const next = new Set(chosen)
    if (!next.delete(id)) next.add(id)
    onChange(next)
  }

  return (
    <fieldset className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <legend className="text-base font-medium">Którzy posłowie głosują?</legend>
        {people.length > 0 && (
          <Button
            type="button"
            variant="ghost"
            onClick={() => onChange(allChosen ? new Set() : new Set(people.map((p) => p.id)))}
          >
            {allChosen ? 'Odznacz wszystkich' : 'Zaznacz wszystkich'}
          </Button>
        )}
      </div>
      {me && (
        <div className="flex items-center gap-4 rounded-lg border bg-card px-4 py-3 opacity-80">
          <Checkbox checked disabled aria-label="Wnioskodawca" />
          <UserAvatar username={me.username} src={me.avatar_url} size="md" />
          <span className="text-metal text-lg font-medium">{me.username} (wnioskodawca)</span>
        </div>
      )}
      {loading && <span className="text-base text-muted-foreground">Ładowanie…</span>}
      {people.map((person) => (
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
      {error && (
        <span role="alert" className="text-base text-destructive">
          {error}
        </span>
      )}
    </fieldset>
  )
}
