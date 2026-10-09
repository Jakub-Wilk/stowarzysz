import type { ReactNode } from 'react'

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { cn } from '@/lib/utils'

export interface SelectOption<V extends string> {
  value: V
  label: string
  /** Shown before the label, in the list and in the field (an emoji, a small icon). */
  icon?: ReactNode
}

interface SelectFieldProps<V extends string> {
  options: SelectOption<V>[]
  /** `null` shows the placeholder (nothing chosen yet). */
  value: V | null
  onChange: (value: V) => void
  /** Pair with `<Label htmlFor>`; or give `aria-label` when there is no visible label. */
  id?: string
  'aria-label'?: string
  placeholder?: string
  /** `icon`: a square trigger showing only the chosen option's icon (the list still shows the
   * labels); its accessible name becomes "{aria-label}: {label}". */
  display?: 'full' | 'icon'
  className?: string
}

function OptionLabel<V extends string>({ option }: { option: SelectOption<V> }) {
  if (option.icon === undefined) return option.label
  return (
    <>
      <span aria-hidden className="text-lg leading-none">
        {option.icon}
      </span>
      {option.label}
    </>
  )
}

/**
 * The shared `Select` (the one in the user panel) at form size: as tall as an `Input`, full
 * width. Use it for every dropdown in a form instead of a native `<select>`. Values are strings;
 * pass ids as `String(id)`.
 */
export function SelectField<V extends string>({
  options,
  value,
  onChange,
  id,
  'aria-label': ariaLabel,
  placeholder,
  display = 'full',
  className,
}: SelectFieldProps<V>) {
  const chosen = options.find((option) => option.value === value)
  const iconOnly = display === 'icon'
  return (
    <Select
      value={value}
      onValueChange={(next) => {
        if (next !== null) onChange(next as V)
      }}
      items={options}
    >
      <SelectTrigger
        id={id}
        aria-label={iconOnly && chosen ? `${ariaLabel}: ${chosen.label}` : ariaLabel}
        className={cn(
          'text-base data-[size=default]:h-12',
          iconOnly ? 'w-16 shrink-0 justify-center gap-0.5 px-2' : 'w-full px-3.5',
          className,
        )}
      >
        <SelectValue placeholder={placeholder}>
          {iconOnly ? (
            <span aria-hidden className="text-2xl leading-none">
              {chosen?.icon ?? '…'}
            </span>
          ) : chosen ? (
            <OptionLabel option={chosen} />
          ) : (
            placeholder
          )}
        </SelectValue>
      </SelectTrigger>
      <SelectContent
        alignItemWithTrigger={false}
        align={iconOnly ? 'start' : 'center'}
        className={cn(iconOnly && 'min-w-60')}
      >
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            <OptionLabel option={option} />
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
