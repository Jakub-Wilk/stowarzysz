import { Ban, CircleSlash } from 'lucide-react'

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { cn } from '@/lib/utils'

const KINDS = {
  veto: { Icon: Ban, label: 'Weto', className: 'bg-negative/15 text-negative' },
  abstain: {
    Icon: CircleSlash,
    label: 'Wstrzymał(a) się',
    className: 'bg-muted text-muted-foreground',
  },
} as const

/**
 * Icon badge for a veto or an abstention. The reason (if any) opens on hover, focus or tap,
 * so it works on touch screens as well.
 */
export function ReasonBadge({ type, reason }: { type: keyof typeof KINDS; reason?: string }) {
  const { Icon, label, className } = KINDS[type]
  return (
    <Popover>
      <PopoverTrigger
        openOnHover
        delay={150}
        aria-label={reason ? `${label}: ${reason}` : label}
        className={cn(
          'flex size-10 shrink-0 items-center justify-center rounded-full outline-none focus-visible:ring-3 focus-visible:ring-ring/50',
          className,
        )}
      >
        <Icon className="size-6" aria-hidden />
      </PopoverTrigger>
      <PopoverContent side="top" className="w-auto max-w-64 px-3 py-2">
        <p className="font-semibold">{label}</p>
        {reason && <p className="text-base break-words whitespace-pre-wrap">{reason}</p>}
      </PopoverContent>
    </Popover>
  )
}
