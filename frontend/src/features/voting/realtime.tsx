import { useQueryClient } from '@tanstack/react-query'
import { Outlet } from 'react-router'

import { emitReaction } from '@/features/voting/reactionBus'
import { REACTION_EMOJI, type ReactionEmoji, type ReactionEvent } from '@/lib/api-types'
import { useEventStream } from '@/lib/use-event-stream'

function isReactionEvent(data: unknown): data is ReactionEvent {
  if (typeof data !== 'object' || data === null) return false
  const d = data as Record<string, unknown>
  return (
    typeof d.poll_id === 'number' &&
    typeof d.user_id === 'number' &&
    REACTION_EMOJI.includes(d.emoji as ReactionEmoji)
  )
}

/**
 * Keeps the app live for signed-in users: poll events only carry ids, so they just trigger a
 * refetch (the API decides what each person may see); reactions go to the open results screen.
 */
export function RealtimeShell() {
  const queryClient = useQueryClient()

  useEventStream((type, data) => {
    if (!type.startsWith('poll.')) return
    void queryClient.invalidateQueries({ queryKey: ['polls'] })
    if (type === 'poll.reaction' && isReactionEvent(data)) emitReaction(data)
  })

  return <Outlet />
}
