import { Check, X } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { formatStake, PARTICIPANT_STATE_LABEL } from '@/features/pacts/format'
import { useDecideJoin } from '@/features/pacts/hooks'
import { getPactKind } from '@/features/pacts/kinds'
import { MutationError } from '@/features/pacts/MutationError'
import type { PactDetail, PactParticipantState } from '@/lib/api-types'
import { cn } from '@/lib/utils'

const GONE: PactParticipantState[] = ['declined', 'rejected', 'withdrawn', 'expired']

/** Everyone in the pact with their side and stake; join requests carry approve/reject buttons. */
export function Participants({ pact }: { pact: PactDetail }) {
  const kind = getPactKind(pact.kind)
  const decide = useDecideJoin(pact.id)
  // people who are out of it sink to the bottom
  const rows = [...pact.participants].sort(
    (a, b) => Number(GONE.includes(a.state)) - Number(GONE.includes(b.state)),
  )

  return (
    <section className="flex flex-col gap-3" aria-label="Uczestnicy">
      <h3 className="text-lg font-semibold">Uczestnicy</h3>
      <ul className="flex flex-col gap-2">
        {rows.map((p) => {
          const stake = formatStake(p)
          const deciding = pact.actions.to_decide.includes(p.id)
          const note = [
            p.role === 'host'
              ? kind.key === 'resolution'
                ? 'zobowiązuje się'
                : 'autor'
              : kind.key === 'resolution'
                ? 'sędzia'
                : null,
            kind.sided && p.side ? `strona „${p.side}”` : null,
            stake ? `stawka: ${stake}` : null,
            p.state !== 'active' && p.state !== 'settled' ? PARTICIPANT_STATE_LABEL[p.state] : null,
          ]
            .filter(Boolean)
            .join(' · ')
          return (
            <li
              key={p.id}
              className={cn(
                'flex items-center gap-3 rounded-lg border bg-card px-4 py-3',
                GONE.includes(p.state) && 'opacity-50',
              )}
            >
              <UserAvatar username={p.user.username} src={p.user.avatar_url} size="md" />
              <div className="flex min-w-0 flex-1 flex-col">
                <span className="text-metal truncate text-lg font-medium">{p.user.username}</span>
                {note && <span className="text-sm text-muted-foreground">{note}</span>}
              </div>
              {deciding && (
                <div className="flex gap-1">
                  <Button
                    size="icon-sm"
                    aria-label={`Przyjmij ${p.user.username}`}
                    disabled={decide.isPending}
                    onClick={() => decide.mutate({ participantId: p.id, approve: true })}
                  >
                    <Check />
                  </Button>
                  <Button
                    size="icon-sm"
                    variant="destructive"
                    aria-label={`Odrzuć ${p.user.username}`}
                    disabled={decide.isPending}
                    onClick={() => decide.mutate({ participantId: p.id, approve: false })}
                  >
                    <X />
                  </Button>
                </div>
              )}
            </li>
          )
        })}
      </ul>
      {pact.actions.to_decide.length > 0 && (
        <p className="text-sm text-muted-foreground">
          {kind.key === 'group_bet'
            ? 'Zmiana stawek wpływa na wypłaty wszystkich, więc każdy uczestnik musi zaakceptować nową osobę.'
            : 'Jako autor decydujesz, kto dołączy.'}
        </p>
      )}
      <MutationError error={decide.error} />
    </section>
  )
}
