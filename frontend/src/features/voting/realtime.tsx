import { useQueryClient } from '@tanstack/react-query'
import { Outlet } from 'react-router'

import { meQueryKey } from '@/features/auth/hooks'
import { SantaThemeSync } from '@/features/secretsanta/SantaThemeSync'
import { santaKey } from '@/features/secretsanta/hooks'
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
    if (type.startsWith('santa.')) {
      void queryClient.invalidateQueries({ queryKey: santaKey })
      return
    }
    if (!type.startsWith('poll.')) return
    void queryClient.invalidateQueries({ queryKey: ['polls'] })
    if (type === 'poll.closed') {
      // a nickname or picture vote may just have changed someone's profile
      void queryClient.invalidateQueries({ queryKey: ['people'] })
      void queryClient.invalidateQueries({ queryKey: meQueryKey })
    }
    if (type === 'poll.reaction' && isReactionEvent(data)) emitReaction(data)
  })

  return (
    <>
      <SantaThemeSync />
      <Outlet />
    </>
  )
}
