import { formErrors } from '@/lib/api-errors'

/** The first failed mutation among `errors`, as a Polish message (nothing when none failed). */
export function MutationError({ error }: { error: unknown }) {
  if (!error) return null
  const parsed = formErrors(error)
  return (
    <span role="alert" className="text-base text-destructive">
      {parsed.general ?? Object.values(parsed.fields).join(' ')}
    </span>
  )
}
