import { Navigate, useParams } from 'react-router'

import { BackLink } from '@/components/layout/BackLink'
import { Button } from '@/components/ui/button'
import { useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { Attachments } from '@/features/pacts/Attachments'
import { ClaimsPanel } from '@/features/pacts/ClaimsPanel'
import { dueLabel, isArchived } from '@/features/pacts/format'
import { usePact } from '@/features/pacts/hooks'
import { getPactKind } from '@/features/pacts/kinds'
import { Participants } from '@/features/pacts/Participants'
import { ResponsePanel } from '@/features/pacts/ResponsePanel'
import { KindChip, StatusChip } from '@/features/pacts/StatusChip'
import { formatDate } from '@/features/secretsanta/format'
import { toneClasses } from '@/features/voting/tone'
import { ApiError } from '@/lib/api'
import type { PactDetail } from '@/lib/api-types'
import { cn } from '@/lib/utils'

const sentence = (text: string) => text.charAt(0).toUpperCase() + text.slice(1)

/** What the pact ended in: one line per settled wager (bets) or the single result. */
function Outcome({ pact }: { pact: PactDetail }) {
  const kind = getPactKind(pact.kind)
  const outcome = pact.outcome
  if (!outcome) return null

  const lines: string[] = []
  if (Array.isArray(outcome.wagers)) {
    for (const w of outcome.wagers as {
      participant_id: number
      result: Record<string, unknown>
    }[]) {
      const wager = pact.participants.find((p) => p.id === w.participant_id) ?? null
      const text = kind.describeResult(w.result, pact, wager)
      lines.push(wager ? `${wager.user.username}: ${text}` : sentence(text))
    }
  } else if (outcome.result && typeof outcome.result === 'object') {
    lines.push(sentence(kind.describeResult(outcome.result as Record<string, unknown>, pact, null)))
  }
  if (lines.length === 0) return null

  return (
    <section
      className={cn(
        'flex flex-col gap-1 rounded-2xl border p-5',
        toneClasses(pact.status === 'void' ? 'neutral' : 'positive').panel,
      )}
      aria-label="Wynik"
    >
      <h3 className="text-lg font-semibold">Wynik</h3>
      {lines.map((line) => (
        <p key={line} className="text-base">
          {line}
        </p>
      ))}
    </section>
  )
}

function PactView({ pact }: { pact: PactDetail }) {
  const { data: me } = useMe()
  const kind = getPactKind(pact.kind)

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-2">
        <h2 className="text-3xl font-semibold break-words">{pact.title}</h2>
        <div className="flex flex-wrap items-center gap-2 text-base text-muted-foreground">
          <UserAvatar username={pact.creator.username} src={pact.creator.avatar_url} size="sm" />
          <span>{pact.creator.username}</span>
          <KindChip label={kind.label} />
          <StatusChip status={pact.status} />
        </div>
        {pact.due_at && (
          <span className="text-sm text-muted-foreground">
            Termin: {formatDate(pact.due_at)}
            {!isArchived(pact.status) && ` (${dueLabel(pact.due_at)})`}
          </span>
        )}
        {pact.is_open && !isArchived(pact.status) && (
          <span className="text-sm text-muted-foreground">
            Otwarty: każdy może poprosić o dołączenie.
          </span>
        )}
      </header>

      <section className="flex flex-col gap-2 rounded-lg border bg-card p-4">
        <h3 className="text-sm font-semibold tracking-wide text-muted-foreground uppercase">
          {kind.key === 'resolution' ? 'Postanowienie' : 'Warunek'}
        </h3>
        <p className="text-lg whitespace-pre-wrap">{pact.condition}</p>
        {pact.notes && (
          <>
            <h3 className="mt-2 text-sm font-semibold tracking-wide text-muted-foreground uppercase">
              Dodatkowe informacje
            </h3>
            <p className="text-base whitespace-pre-wrap">{pact.notes}</p>
          </>
        )}
      </section>

      <Attachments pact={pact} myId={me?.id} />
      <Outcome pact={pact} />
      <ResponsePanel pact={pact} />
      <ClaimsPanel pact={pact} />
      <Participants pact={pact} />
    </div>
  )
}

export function PactDetailPage() {
  const { id } = useParams()
  const pactId = Number(id)
  const valid = Number.isInteger(pactId)
  const { data: pact, error, isPending, refetch } = usePact(valid ? pactId : 0)

  if (!valid || (error instanceof ApiError && error.status === 404)) {
    return <Navigate to="/pacts" replace />
  }

  return (
    <>
      <BackLink to="/pacts">Zakłady</BackLink>
      {isPending && <span className="text-sm text-muted-foreground">Ładowanie…</span>}
      {error && !isPending && (
        <div className="flex flex-col items-start gap-2">
          <span role="alert" className="text-sm text-destructive">
            Nie udało się wczytać zakładu.
          </span>
          <Button variant="outline" onClick={() => refetch()}>
            Spróbuj ponownie
          </Button>
        </div>
      )}
      {pact && <PactView pact={pact} />}
    </>
  )
}
