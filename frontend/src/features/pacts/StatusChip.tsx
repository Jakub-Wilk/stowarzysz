import { STATUS_LABEL, STATUS_TONE } from '@/features/pacts/format'
import { toneClasses } from '@/features/voting/tone'
import type { PactStatus } from '@/lib/api-types'
import { cn } from '@/lib/utils'

export function StatusChip({ status }: { status: PactStatus }) {
  return (
    <span
      className={cn(
        'rounded-full px-2.5 py-0.5 text-xs font-bold uppercase',
        toneClasses(STATUS_TONE[status]).chip,
      )}
    >
      {STATUS_LABEL[status]}
    </span>
  )
}

export function KindChip({ label }: { label: string }) {
  return (
    <span className="rounded-full border px-2.5 py-0.5 text-xs font-semibold uppercase">
      {label}
    </span>
  )
}
