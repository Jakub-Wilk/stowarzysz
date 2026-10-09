import { ChevronRight } from 'lucide-react'
import { Link } from 'react-router'

import { useFlashOnChange } from '@/components/motion/useFlashOnChange'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { dueLabel, isArchived } from '@/features/pacts/format'
import { getPactKind } from '@/features/pacts/kinds'
import { KindChip, StatusChip } from '@/features/pacts/StatusChip'
import type { PactListItem } from '@/lib/api-types'
import { PARTICIPANT_FORMS, plural } from '@/lib/plural'
import { cn } from '@/lib/utils'

/** One pact in the lists: who made it, what kind, where it stands and whether it needs you. */
export function PactRow({ pact }: { pact: PactListItem }) {
  const flashing = useFlashOnChange(`${pact.status}:${pact.active_count}`)
  const needsYou = pact.my.state === 'invited' && !isArchived(pact.status)
  const dueText =
    pact.due_at && !isArchived(pact.status) ? `Termin: ${dueLabel(pact.due_at)}` : null

  return (
    <Link
      to={`/pacts/${pact.id}`}
      className={cn(
        'group flex items-center gap-4 rounded-lg border bg-card p-4 transition-all duration-(--duration-base) ease-(--ease-out-soft) hover:-translate-y-0.5 hover:bg-accent hover:shadow-lg focus-visible:border-ring focus-visible:outline-none',
        flashing && 'animate-flash',
      )}
    >
      <UserAvatar username={pact.creator.username} src={pact.creator.avatar_url} size="md" />
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <span className="text-metal line-clamp-2 text-lg leading-snug font-semibold break-words">
          {pact.title}
        </span>
        <span className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-muted-foreground">
          <KindChip label={getPactKind(pact.kind).label} />
          <StatusChip status={pact.status} />
          <span>{pact.creator.username}</span>
          <span>
            {pact.active_count} {plural(pact.active_count, PARTICIPANT_FORMS)}
          </span>
          {dueText && <span>{dueText}</span>}
          {needsYou && (
            <span className="font-semibold text-foreground">Czeka na Twoją odpowiedź</span>
          )}
        </span>
      </div>
      <ChevronRight
        className="size-5 shrink-0 transition-transform group-hover:translate-x-1"
        aria-hidden
      />
    </Link>
  )
}
