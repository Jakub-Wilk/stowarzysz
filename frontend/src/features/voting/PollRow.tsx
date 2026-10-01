import { ChevronRight } from 'lucide-react'
import { Link } from 'react-router'

import { useFlashOnChange } from '@/components/motion/useFlashOnChange'

import { UserAvatar } from '@/features/auth/UserAvatar'
import { getKindUI } from '@/features/voting/kinds'
import type { PollListItem } from '@/lib/api-types'
import { cn } from '@/lib/utils'

/** One vote in the lists: active ones carry an "Active" label, finished ones their outcome. */
export function PollRow({ poll }: { poll: PollListItem }) {
  const isOpen = poll.status === 'open'
  const ResultChip = getKindUI(poll.kind)?.ResultChip
  const needsYou = isOpen && poll.my.participating && !poll.my.has_voted
  const flashing = useFlashOnChange(`${poll.status}:${poll.voted_count}`)

  return (
    <Link
      to={`/voting/${poll.id}`}
      className={cn(
        'flex items-center gap-4 rounded-lg border bg-card p-4 group transition-all duration-(--duration-base) ease-(--ease-out-soft) hover:-translate-y-0.5 hover:bg-accent hover:shadow-lg focus-visible:border-ring focus-visible:outline-none',
        flashing && 'animate-flash',
      )}
    >
      <UserAvatar username={poll.creator.username} src={poll.creator.avatar_url} size="md" />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="text-metal line-clamp-2 text-lg leading-snug font-semibold break-words">
          {poll.title}
        </span>
        <span className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-muted-foreground">
          <span>{poll.creator.username}</span>
          {isOpen && (
            <>
              <span className="animate-glow rounded-full bg-primary px-2.5 py-0.5 font-bold text-primary-foreground uppercase">
                Active
              </span>
              <span>
                {poll.voted_count}/{poll.participant_count} voted
              </span>
              {needsYou && (
                <span className="font-semibold text-foreground">Your vote is needed</span>
              )}
            </>
          )}
        </span>
      </div>
      {!isOpen && poll.result && ResultChip && <ResultChip result={poll.result} />}
      <ChevronRight
        className="size-5 shrink-0 transition-transform group-hover:translate-x-1"
        aria-hidden
      />
    </Link>
  )
}
