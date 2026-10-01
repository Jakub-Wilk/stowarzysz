import { ChevronRight } from 'lucide-react'
import { Link } from 'react-router'

import { UserAvatar } from '@/features/auth/UserAvatar'
import { getKindUI } from '@/features/voting/kinds'
import type { PollListItem } from '@/lib/api-types'

/** One vote in the lists: active ones carry an "Active" label, finished ones their outcome. */
export function PollRow({ poll }: { poll: PollListItem }) {
  const isOpen = poll.status === 'open'
  const ResultChip = getKindUI(poll.kind)?.ResultChip
  const needsYou = isOpen && poll.my.participating && !poll.my.has_voted

  return (
    <Link
      to={`/voting/${poll.id}`}
      className="flex items-center gap-4 rounded-lg border bg-card p-4 transition-colors hover:bg-accent focus-visible:border-ring focus-visible:outline-none"
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
              <span className="rounded-full bg-primary px-2.5 py-0.5 font-bold text-primary-foreground uppercase">
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
      <ChevronRight className="size-5 shrink-0" aria-hidden />
    </Link>
  )
}
