import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { formatScore } from '@/features/voting/format'
import { ScoreDial } from '@/features/voting/kinds/ScoreDial'
import { toneClasses } from '@/features/voting/tone'
import type { Ballot, PollDetail, PollResult } from '@/lib/api-types'
import { cn } from '@/lib/utils'

import type { BallotInputProps } from './types'

interface ScoreResult {
  votes_cast: number
  veto_count: number
  vetoed: boolean
  score: number | null
}

function readResult(result: PollResult | null): ScoreResult | null {
  if (!result) return null
  const { votes_cast, veto_count, vetoed, score } = result
  if (typeof votes_cast !== 'number' || typeof veto_count !== 'number') return null
  return {
    votes_cast,
    veto_count,
    vetoed: vetoed === true,
    score: typeof score === 'number' ? score : null,
  }
}

const readValue = (ballot: Ballot | null | undefined): number | null =>
  typeof ballot?.value === 'number' ? ballot.value : null

function scoreTone(value: number) {
  return value > 0 ? 'positive' : value < 0 ? 'negative' : 'neutral'
}

export function ScoreBallotInput({ value, pending, onSubmit }: BallotInputProps) {
  const saved = readValue(value)
  const [picked, setPicked] = useState<number | null>(saved)

  return (
    <div className="flex flex-col gap-4">
      <ScoreDial value={picked} onChange={setPicked} disabled={pending} />
      <Button
        disabled={picked === null || picked === saved || pending}
        onClick={() => picked !== null && onSubmit({ value: picked })}
      >
        {saved === null ? 'Cast vote' : 'Change vote'}
      </Button>
    </div>
  )
}

export function ScoreResultDisplay({ poll }: { poll: PollDetail }) {
  const result = readResult(poll.result)
  const tone = toneClasses(poll.result?.tone)

  return (
    <div className="flex flex-col gap-4">
      <div className={cn('flex flex-col items-center gap-2 rounded-2xl border p-6', tone.panel)}>
        {result?.vetoed && (
          <div
            role="status"
            className="rounded-md bg-negative px-4 py-1 text-lg font-black tracking-[0.3em] text-background uppercase"
          >
            Vetoed
          </div>
        )}
        {result && result.score !== null ? (
          <div className={cn('text-7xl font-black tabular-nums', tone.text)}>
            {formatScore(result.score)}
          </div>
        ) : (
          <div className="text-xl font-semibold">No votes cast</div>
        )}
        {result && (
          <div className="text-base text-muted-foreground">
            {result.votes_cast} of {poll.participant_count} voted
            {result.veto_count > 1 && ` · ${result.veto_count} vetoes`}
          </div>
        )}
      </div>

      <ul className="flex flex-col gap-2">
        {poll.participants.map(({ user, has_voted, ballot, vetoed }) => {
          const value = readValue(ballot)
          return (
            <li
              key={user.id}
              className="flex items-center gap-3 rounded-lg border bg-card px-4 py-3"
            >
              <UserAvatar username={user.username} src={user.avatar_url} size="md" />
              <span className="text-metal flex-1 truncate text-lg font-medium">
                {user.username}
              </span>
              {vetoed && (
                <span className="rounded-full bg-negative/15 px-2 py-0.5 text-xs font-bold text-negative uppercase">
                  Veto
                </span>
              )}
              {has_voted && value !== null ? (
                <span
                  className={cn(
                    'w-12 text-right text-2xl font-bold tabular-nums',
                    toneClasses(scoreTone(value)).text,
                  )}
                >
                  {value > 0 ? `+${value}` : value}
                </span>
              ) : (
                <span className="text-base text-muted-foreground">didn&apos;t vote</span>
              )}
            </li>
          )
        })}
      </ul>
    </div>
  )
}

export function ScoreResultChip({ result }: { result: PollResult }) {
  const parsed = readResult(result)
  const tone = toneClasses(result.tone)
  return (
    <span className="flex items-center gap-1.5">
      {parsed?.vetoed && (
        <span className="rounded-full bg-negative px-2 py-0.5 text-xs font-black text-background uppercase">
          Vetoed
        </span>
      )}
      <span className={cn('rounded-full px-2.5 py-0.5 text-sm font-bold tabular-nums', tone.chip)}>
        {parsed && parsed.score !== null ? formatScore(parsed.score) : '—'}
      </span>
    </span>
  )
}
