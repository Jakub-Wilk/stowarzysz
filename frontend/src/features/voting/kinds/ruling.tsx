import { useState } from 'react'
import { Link } from 'react-router'

import { Button } from '@/components/ui/button'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { getPactKind } from '@/features/pacts/kinds'
import { usePact } from '@/features/pacts/hooks'
import { toneClasses } from '@/features/voting/tone'
import type { Ballot, PollDetail, PollResult } from '@/lib/api-types'
import { cn } from '@/lib/utils'

import type { BallotInputProps } from './types'

const readChoice = (ballot: Ballot | null | undefined): boolean | null =>
  typeof ballot?.upheld === 'boolean' ? ballot.upheld : null

/** The disputed claim, worded like on the pact page, with a link to the pact. */
export function RulingProposal({ poll }: { poll: PollDetail }) {
  const pactId = Number(poll.config.pact_id)
  const claim = (poll.config.claim ?? {}) as Record<string, unknown>
  const pact = usePact(Number.isInteger(pactId) ? pactId : 0)
  const proposal = pact.data?.proposals.find((p) => p.id === Number(poll.config.proposal_id))
  const wager = pact.data?.participants.find((p) => p.id === proposal?.wager_id) ?? null
  const text = pact.data
    ? getPactKind(pact.data.kind).describeResult(claim, pact.data, wager)
    : null

  return (
    <section className="flex flex-col gap-2 rounded-lg border bg-card p-4">
      <p className="text-base">
        Strony zakładu nie zgadzają się co do wyniku. Czy ten wynik jest słuszny?
      </p>
      {text && <p className="text-xl font-semibold">{text}</p>}
      <Button
        variant="outline"
        className="self-start"
        nativeButton={false}
        render={<Link to={`/pacts/${pactId}`} />}
      >
        Zobacz zakład
      </Button>
    </section>
  )
}

export function RulingBallotInput({ value, pending, onSubmit }: BallotInputProps) {
  const saved = readChoice(value)
  const [picked, setPicked] = useState<boolean | null>(saved)
  return (
    <div className="flex flex-col gap-3">
      <div className="grid grid-cols-2 gap-2" role="group" aria-label="Rozstrzygnięcie">
        <Button
          variant={picked === true ? 'default' : 'outline'}
          aria-pressed={picked === true}
          disabled={pending}
          onClick={() => setPicked(true)}
        >
          Wynik słuszny
        </Button>
        <Button
          variant={picked === false ? 'default' : 'outline'}
          aria-pressed={picked === false}
          disabled={pending}
          onClick={() => setPicked(false)}
        >
          Wynik nietrafiony
        </Button>
      </div>
      <Button
        disabled={picked === null || picked === saved || pending}
        onClick={() => picked !== null && onSubmit({ upheld: picked })}
      >
        {saved === null ? 'Oddaj głos' : 'Zmień głos'}
      </Button>
    </div>
  )
}

export function RulingResultDisplay({ poll }: { poll: PollDetail }) {
  const upheld = poll.result?.approved === true
  const tone = toneClasses(upheld ? 'positive' : 'negative')
  const applied = poll.result?.applied
  return (
    <div className="flex flex-col gap-4">
      <div className={cn('flex flex-col items-center gap-2 rounded-2xl border p-6', tone.panel)}>
        <div className={cn('text-3xl font-black', tone.text)}>
          {upheld ? 'Wynik podtrzymany' : 'Wynik odrzucony'}
        </div>
        <div className="text-base text-muted-foreground">
          Za: {String(poll.result?.upheld ?? 0)} · Przeciw: {String(poll.result?.rejected ?? 0)}
        </div>
        {upheld && applied === false && (
          <div className="text-base text-destructive">
            Nie udało się zastosować rozstrzygnięcia.
          </div>
        )}
        {!upheld && (
          <div className="text-base text-muted-foreground">
            Strony mogą podać nowy wynik w zakładzie.
          </div>
        )}
      </div>
      <ul className="flex flex-col gap-2">
        {poll.participants.map(({ user, has_voted, ballot }) => {
          const choice = readChoice(ballot)
          return (
            <li
              key={user.id}
              className="flex items-center gap-3 rounded-lg border bg-card px-4 py-3"
            >
              <UserAvatar username={user.username} src={user.avatar_url} size="md" />
              <span className="text-metal flex-1 truncate text-lg font-medium">
                {user.username}
              </span>
              {has_voted && choice !== null ? (
                <span
                  className={cn(
                    'text-base font-bold',
                    toneClasses(choice ? 'positive' : 'negative').text,
                  )}
                >
                  {choice ? 'Słuszny' : 'Nietrafiony'}
                </span>
              ) : (
                <span className="text-base text-muted-foreground">brak głosu</span>
              )}
            </li>
          )
        })}
      </ul>
    </div>
  )
}

export function RulingResultChip({ result }: { result: PollResult }) {
  const upheld = result.approved === true
  return (
    <span
      className={cn(
        'rounded-full px-2.5 py-0.5 text-sm font-bold',
        toneClasses(upheld ? 'positive' : 'negative').chip,
      )}
    >
      {upheld ? 'Podtrzymano' : 'Odrzucono'}
    </span>
  )
}
