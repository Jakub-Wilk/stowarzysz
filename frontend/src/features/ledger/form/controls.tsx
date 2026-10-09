import type { ReactNode } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { todayInput } from '@/features/ledger/format'
import { cn } from '@/lib/utils'

/** A part of an entry form: a small uppercase heading (like the feed's day headings), with an
 * optional control on its right, then the content. */
export function FormSection({
  title,
  action,
  children,
}: {
  title: string
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="flex flex-col gap-3" aria-label={title}>
      <div className="flex min-h-10 items-center justify-between gap-2">
        <h3 className="text-sm font-semibold tracking-wide text-muted-foreground uppercase">
          {title}
        </h3>
        {action}
      </div>
      {children}
    </section>
  )
}

/** One choice out of a few, as equal buttons side by side. */
export function Segmented<V extends string>({
  options,
  value,
  onChange,
  label,
  size = 'default',
}: {
  options: readonly (readonly [V, string])[]
  value: V
  onChange: (value: V) => void
  label: string
  size?: 'default' | 'sm'
}) {
  return (
    <div
      role="group"
      aria-label={label}
      className="grid gap-1 rounded-xl border bg-card p-1"
      style={{ gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))` }}
    >
      {options.map(([key, text]) => (
        <Button
          key={key}
          type="button"
          size={size}
          variant={value === key ? 'default' : 'ghost'}
          aria-pressed={value === key}
          className={cn('min-w-0 px-1', value !== key && 'no-underline')}
          onClick={() => onChange(key)}
        >
          <span className="truncate">{text}</span>
        </Button>
      ))}
    </div>
  )
}

/** "Kiedy" with a date picker on one line; no future dates. */
export function DateField({
  id,
  value,
  onChange,
}: {
  id: string
  value: string
  onChange: (value: string) => void
}) {
  return (
    <div className="flex items-center justify-between gap-3">
      <label htmlFor={id} className="text-base font-medium">
        Kiedy
      </label>
      <Input
        id={id}
        type="date"
        className="w-44"
        max={todayInput()}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  )
}

/** The form's errors, right above its submit button. */
export function FormProblem({ children }: { children: string | null }) {
  if (!children) return null
  return (
    <span role="alert" className="text-base text-destructive">
      {children}
    </span>
  )
}
