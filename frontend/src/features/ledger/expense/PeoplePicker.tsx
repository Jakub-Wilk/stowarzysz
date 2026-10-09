import { Button } from '@/components/ui/button'
import { UserAvatar } from '@/features/auth/UserAvatar'
import type { UserBrief } from '@/lib/api-types'
import { cn } from '@/lib/utils'

interface PeoplePickerProps {
  people: UserBrief[]
  selected: ReadonlySet<number>
  onChange: (next: Set<number>) => void
  /** Avatars only (a receipt line); otherwise avatars with names. */
  compact?: boolean
  label: string
}

/** Toggle who shares a cost, plus a shortcut for everyone. */
export function PeoplePicker({ people, selected, onChange, compact, label }: PeoplePickerProps) {
  const everyone = people.length > 0 && people.every((p) => selected.has(p.id))
  const toggle = (id: number) => {
    const next = new Set(selected)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    onChange(next)
  }

  return (
    <div role="group" aria-label={label} className="flex flex-wrap items-center gap-1.5">
      {people.map((person) => {
        const on = selected.has(person.id)
        return (
          <button
            key={person.id}
            type="button"
            aria-pressed={on}
            title={person.username}
            aria-label={compact ? person.username : undefined}
            onClick={() => toggle(person.id)}
            className={cn(
              'flex items-center gap-2 rounded-full border transition-[opacity,transform] duration-(--duration-fast) active:scale-95',
              compact ? 'p-0.5' : 'py-1 pr-3 pl-1',
              on ? 'border-primary bg-primary/10' : 'opacity-50 grayscale',
            )}
          >
            <UserAvatar username={person.username} src={person.avatar_url} size="sm" />
            {!compact && <span className="text-base">{person.username}</span>}
          </button>
        )
      })}
      <Button
        type="button"
        variant="ghost"
        size="sm"
        className="h-8 px-2"
        onClick={() => onChange(everyone ? new Set() : new Set(people.map((p) => p.id)))}
      >
        {everyone ? 'nikt' : 'wszyscy'}
      </Button>
    </div>
  )
}
