import { Check } from 'lucide-react'
import { useEffect, useState } from 'react'
import { Navigate, useParams } from 'react-router'

import { BackLink } from '@/components/layout/BackLink'
import { useCountUp } from '@/components/motion/useCountUp'
import { Button } from '@/components/ui/button'
import { useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { ConfirmDialog } from '@/features/voting/ConfirmDialog'
import { formatWhen } from '@/features/voting/dates'
import {
  useCastBallot,
  useClosePoll,
  usePoll,
  useSendReaction,
  useVeto,
} from '@/features/voting/hooks'
import { getKindUI } from '@/features/voting/kinds'
import { subscribeReactions } from '@/features/voting/reactionBus'
import { useSmoke } from '@/features/voting/SmokeLayer'
import { ApiError } from '@/lib/api'
import { formErrors } from '@/lib/api-errors'
import { REACTION_EMOJI, type PollDetail } from '@/lib/api-types'
import { cn } from '@/lib/utils'

function Progress({ poll }: { poll: PollDetail }) {
  const pct = poll.participant_count ? (poll.voted_count / poll.participant_count) * 100 : 0
  // Start empty so the bar fills in on mount, then follows live updates.
  const [shown, setShown] = useState(0)
  useEffect(() => {
    const frame = requestAnimationFrame(() => setShown(pct))
    return () => cancelAnimationFrame(frame)
  }, [pct])
  const voted = useCountUp(poll.voted_count, 500)
  return (
    <section className="flex flex-col gap-3" aria-label="Progress">
      <div className="flex items-center justify-between text-base">
        <span>
          {voted} of {poll.participant_count} voted
        </span>
        <span className="text-sm text-muted-foreground">Votes are hidden until the end</span>
      </div>
      <div className="h-3 overflow-hidden rounded-full bg-muted">
        <div
          className="h-full bg-primary transition-[width] duration-700 ease-(--ease-out-soft)"
          style={{ width: `${shown}%` }}
        />
      </div>
      <ul className="flex flex-wrap gap-3">
        {poll.participants.map(({ user, has_voted }) => (
          <li
            key={user.id}
            title={`${user.username}${has_voted ? ' has voted' : ' hasn’t voted yet'}`}
            className={cn('relative flex flex-col items-center gap-1', !has_voted && 'opacity-50')}
          >
            <UserAvatar username={user.username} src={user.avatar_url} size="md" />
            {has_voted && (
              <span className="absolute -top-1 -right-1 animate-pop-in rounded-full bg-positive p-0.5 text-background">
                <Check className="size-4 stroke-3!" aria-hidden />
              </span>
            )}
            <span className="max-w-20 truncate text-sm">{user.username}</span>
          </li>
        ))}
      </ul>
    </section>
  )
}

function OpenVote({ poll }: { poll: PollDetail }) {
  const kind = getKindUI(poll.kind)
  const cast = useCastBallot(poll.id)
  const veto = useVeto(poll.id)
  const close = useClosePoll(poll.id)
  const [confirm, setConfirm] = useState<'veto' | 'close' | null>(null)

  const error = [cast, veto, close].find((m) => m.isError)?.error
  const BallotInput = kind?.BallotInput

  return (
    <div className="flex flex-col gap-6">
      <Progress poll={poll} />

      {poll.can_vote && BallotInput && (
        <section className="flex flex-col gap-4">
          <h3 className="text-lg font-semibold">Your vote</h3>
          {poll.my.vetoed ? (
            <p className="rounded-lg border border-negative/60 bg-negative/10 p-4 text-base">
              You vetoed this vote. It counts as the lowest possible score.
            </p>
          ) : (
            <BallotInput
              // remount when the server's copy changes so the picker follows other devices
              key={JSON.stringify(poll.my_ballot)}
              value={poll.my_ballot}
              pending={cast.isPending}
              onSubmit={(ballot) => cast.mutate(ballot)}
            />
          )}
          {!poll.my.vetoed && (
            <Button variant="destructive" className="w-full" onClick={() => setConfirm('veto')}>
              Veto
            </Button>
          )}
        </section>
      )}

      {!poll.my.participating && (
        <p className="rounded-lg border bg-card p-4 text-base">
          You&apos;re not part of this vote. You can follow along and see the result when it ends.
        </p>
      )}

      {poll.can_close && (
        <section className="flex flex-col gap-2 border-t pt-4">
          <p className="text-base text-muted-foreground">
            You called this vote, so you can end it before everyone has voted. Only the votes cast
            so far count.
          </p>
          <Button variant="outline" className="w-full" onClick={() => setConfirm('close')}>
            End vote now
          </Button>
        </section>
      )}

      {error && (
        <span role="alert" className="text-base text-destructive">
          {formErrors(error).general ?? Object.values(formErrors(error).fields).join(' ')}
        </span>
      )}

      <ConfirmDialog
        open={confirm === 'veto'}
        onOpenChange={(open) => !open && setConfirm(null)}
        title="Veto this vote?"
        description="A veto counts as the lowest score and marks the result as vetoed for everyone. You can't take it back."
        confirmLabel="Veto"
        pending={veto.isPending}
        onConfirm={() => veto.mutate(undefined, { onSettled: () => setConfirm(null) })}
      />
      <ConfirmDialog
        open={confirm === 'close'}
        onOpenChange={(open) => !open && setConfirm(null)}
        title="End the vote now?"
        description={`Only the ${poll.voted_count} vote${poll.voted_count === 1 ? '' : 's'} cast so far will count, and nobody else will be able to vote.`}
        confirmLabel="End vote"
        pending={close.isPending}
        onConfirm={() => close.mutate(undefined, { onSettled: () => setConfirm(null) })}
      />
    </div>
  )
}

function ClosedVote({ poll }: { poll: PollDetail }) {
  const { data: me } = useMe()
  const ResultDisplay = getKindUI(poll.kind)?.ResultDisplay
  const react = useSendReaction(poll.id)
  const { spawn, layer } = useSmoke()
  const myId = me?.id

  // Reactions from everyone else looking at this result.
  useEffect(
    () =>
      subscribeReactions((event) => {
        if (event.poll_id === poll.id && event.user_id !== myId) spawn(event.emoji)
      }),
    [poll.id, myId, spawn],
  )

  return (
    <div className="flex flex-col gap-6">
      {ResultDisplay && <ResultDisplay poll={poll} />}
      <div
        className="mx-auto grid w-full max-w-md grid-cols-5 gap-2"
        role="group"
        aria-label="Reactions"
      >
        {REACTION_EMOJI.map((emoji) => (
          <button
            key={emoji}
            type="button"
            aria-label={`React ${emoji}`}
            onClick={() => {
              spawn(emoji) // instant for you; everyone else gets it over the live stream
              react.mutate(emoji)
            }}
            className="flex aspect-square w-full items-center justify-center rounded-full border bg-card text-4xl transition-transform duration-(--duration-fast) ease-(--ease-spring) hover:-translate-y-1 hover:scale-110 active:scale-90"
          >
            {emoji}
          </button>
        ))}
      </div>
      {react.isError && (
        <span role="alert" className="text-center text-sm text-destructive">
          {formErrors(react.error).general ?? 'Couldn’t send that reaction.'}
        </span>
      )}
      {layer}
    </div>
  )
}

export function VoteDetailPage() {
  const { id } = useParams()
  const pollId = Number(id)
  const valid = Number.isInteger(pollId)
  const { data: poll, error, isPending, refetch } = usePoll(valid ? pollId : 0)

  if (!valid || (error instanceof ApiError && error.status === 404)) {
    return <Navigate to="/voting" replace />
  }

  return (
    <>
      <BackLink to="/voting">Voting</BackLink>
      {isPending && <span className="text-sm text-muted-foreground">Loading…</span>}
      {error && !isPending && (
        <div className="flex flex-col items-start gap-2">
          <span role="alert" className="text-sm text-destructive">
            Couldn&apos;t load this vote.
          </span>
          <Button variant="outline" onClick={() => refetch()}>
            Retry
          </Button>
        </div>
      )}
      {poll && (
        <div className="flex flex-col gap-6">
          <header className="flex flex-col gap-2">
            <h2 className="text-3xl font-semibold">{poll.title}</h2>
            <div className="flex flex-wrap items-center gap-2 text-base text-muted-foreground">
              <UserAvatar
                username={poll.creator.username}
                src={poll.creator.avatar_url}
                size="sm"
              />
              <span>
                {poll.creator.username} · {formatWhen(poll.created_at)}
              </span>
              {poll.status === 'open' && (
                <span className="rounded-full bg-primary px-2.5 py-0.5 text-sm font-bold text-primary-foreground uppercase">
                  Active
                </span>
              )}
            </div>
          </header>
          {poll.status === 'open' ? <OpenVote poll={poll} /> : <ClosedVote poll={poll} />}
        </div>
      )}
    </>
  )
}
