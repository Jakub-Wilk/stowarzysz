import { SelectField } from '@/components/SelectField'
import { UserAvatar } from '@/features/auth/UserAvatar'
import type { UserBrief } from '@/lib/api-types'

/** Pick one member, shown with their avatar; the signed-in user reads "(Ty)". */
export function PersonSelect({
  people,
  myId,
  value,
  onChange,
  id,
  'aria-label': ariaLabel,
  placeholder = 'Wybierz…',
}: {
  people: UserBrief[]
  myId: number | undefined
  value: number | null
  onChange: (id: number) => void
  id?: string
  'aria-label'?: string
  placeholder?: string
}) {
  return (
    <SelectField
      id={id}
      aria-label={ariaLabel}
      placeholder={placeholder}
      value={value === null ? null : String(value)}
      onChange={(next) => onChange(Number(next))}
      options={people.map((p) => ({
        value: String(p.id),
        label: p.id === myId ? `${p.username} (Ty)` : p.username,
        icon: <UserAvatar username={p.username} src={p.avatar_url} size="sm" />,
      }))}
    />
  )
}
