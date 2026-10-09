import { useState } from 'react'
import { Link } from 'react-router'

import { Button } from '@/components/ui/button'
import { useClaimAction, useProposeOutcome } from '@/features/pacts/hooks'
import { getPactKind } from '@/features/pacts/kinds'
import { VOID_CHOICE, type OutcomeChoice } from '@/features/pacts/kinds/types'
import { MutationError } from '@/features/pacts/MutationError'
import { ConfirmDialog } from '@/features/voting/ConfirmDialog'
import { toneClasses } from '@/features/voting/tone'
import type { PactDetail, PactParticipant, PactProposal } from '@/lib/api-types'
import { cn } from '@/lib/utils'

function wagerOf(pact: PactDetail, proposal: PactProposal): PactParticipant | null {
  return pact.participants.find((p) => p.id === proposal.wager_id) ?? null
}

function Claim({ pact, proposal }: { pact: PactDetail; proposal: PactProposal }) {
  const kind = getPactKind(pact.kind)
  const action = useClaimAction(pact.id)
  const [escalating, setEscalating] = useState(false)
  const wager = wagerOf(pact, proposal)
  const { actions } = pact
  const mustConfirm = actions.to_confirm.includes(proposal.id)
  const canEscalate = actions.can_escalate.includes(proposal.id)
  const disputed = proposal.state === 'disputed'

  return (
    <li
      className={cn(
        'flex flex-col gap-3 rounded-lg border p-4',
        disputed ? toneClasses('negative').panel : 'bg-card',
      )}
    >
      <p className="text-base">
        <strong>{proposal.proposed_by.username}</strong> twierdzi:{' '}
        {kind.describeResult(proposal.result, pact, wager)}
        {wager && kind.wagerBased && ` (zakład z ${wager.user.username})`}
      </p>
      {disputed ? (
        <>
          <p className="text-sm text-muted-foreground">
            Wynik jest sporny. Strony mogą podać nowy albo oddać sprawę pod głosowanie posłów spoza
            zakładu.
          </p>
          {proposal.ruling_poll_id !== null && (
            <Button
              variant="outline"
              nativeButton={false}
              render={<Link to={`/voting/${proposal.ruling_poll_id}`} />}
            >
              Zobacz głosowanie w Sejmiku
            </Button>
          )}
          {canEscalate && proposal.ruling_poll_id === null && (
            <>
              <Button variant="outline" onClick={() => setEscalating(true)}>
                Oddaj pod głosowanie w Sejmiku
              </Button>
              <ConfirmDialog
                open={escalating}
                onOpenChange={setEscalating}
                title="Oddać spór pod głosowanie?"
                description="Posłowie, którzy nie biorą udziału w zakładzie, zagłosują, czy ten wynik jest słuszny. Przy remisie albo braku głosów wynik zostaje odrzucony."
                confirmLabel="Oddaj pod głosowanie"
                destructive={false}
                pending={action.isPending}
                onConfirm={() =>
                  action.mutate(
                    { proposalId: proposal.id, action: 'escalate' },
                    { onSettled: () => setEscalating(false) },
                  )
                }
              />
            </>
          )}
        </>
      ) : mustConfirm ? (
        <div className="flex gap-2">
          <Button
            className="flex-1"
            disabled={action.isPending}
            onClick={() => action.mutate({ proposalId: proposal.id, action: 'confirm' })}
          >
            Potwierdzam
          </Button>
          <Button
            className="flex-1"
            variant="outline"
            disabled={action.isPending}
            onClick={() => action.mutate({ proposalId: proposal.id, action: 'dispute' })}
          >
            Nie zgadzam się
          </Button>
        </div>
      ) : (
        <p className="text-sm text-muted-foreground">Czeka na potwierdzenie drugiej strony.</p>
      )}
      <MutationError error={action.error} />
    </li>
  )
}

function ProposeForm({ pact }: { pact: PactDetail }) {
  const kind = getPactKind(pact.kind)
  const propose = useProposeOutcome(pact.id)
  const wagers = pact.participants.filter(
    (p) =>
      p.role === 'opponent' &&
      p.state === 'active' &&
      (pact.my.role === 'host' || p.id === pact.my.participant_id),
  )
  const [wagerId, setWagerId] = useState<number | null>(wagers[0]?.id ?? null)
  const [choice, setChoice] = useState<OutcomeChoice | null>(null)
  const wager = wagers.find((w) => w.id === wagerId) ?? null
  const choices = [...kind.outcomeChoices(pact, wager), VOID_CHOICE]

  if (kind.wagerBased && wagers.length === 0) return null

  return (
    <section className="flex flex-col gap-3 rounded-lg border bg-card p-4">
      <h3 className="text-lg font-semibold">Podaj wynik</h3>
      {kind.wagerBased && wagers.length > 1 && (
        <div className="flex flex-wrap gap-2" role="group" aria-label="Zakład">
          {wagers.map((w) => (
            <Button
              key={w.id}
              variant={wagerId === w.id ? 'default' : 'outline'}
              aria-pressed={wagerId === w.id}
              onClick={() => {
                setWagerId(w.id)
                setChoice(null)
              }}
            >
              z {w.user.username}
            </Button>
          ))}
        </div>
      )}
      {kind.wagerBased && wagers.length === 1 && (
        <p className="text-sm text-muted-foreground">Zakład z {wagers[0].user.username}.</p>
      )}
      <div className="flex flex-col gap-2" role="group" aria-label="Wynik">
        {choices.map((c) => (
          <Button
            key={c.label}
            variant={choice?.label === c.label ? 'default' : 'outline'}
            className="h-auto justify-start py-3 text-left whitespace-normal"
            aria-pressed={choice?.label === c.label}
            onClick={() => setChoice(c)}
          >
            {c.label}
          </Button>
        ))}
      </div>
      <Button
        disabled={!choice || propose.isPending}
        onClick={() =>
          choice &&
          propose.mutate(
            { wagerId: kind.wagerBased ? wagerId : null, result: choice.result },
            { onSuccess: () => setChoice(null) },
          )
        }
      >
        {propose.isPending ? 'Wysyłanie…' : 'Wyślij do potwierdzenia'}
      </Button>
      <p className="text-sm text-muted-foreground">
        {kind.wagerBased
          ? 'Wynik zacznie obowiązywać, gdy druga strona go potwierdzi.'
          : 'Wynik zacznie obowiązywać, gdy potwierdzą go wszyscy pozostali uczestnicy.'}
      </p>
      <MutationError error={propose.error} />
    </section>
  )
}

/** Claims about how the pact ended (to confirm, dispute or escalate) and the form to make one. */
export function ClaimsPanel({ pact }: { pact: PactDetail }) {
  const running = pact.status === 'active' || pact.status === 'awaiting_result'
  const participating = pact.my.state === 'active'
  if (!running) return null

  return (
    <section className="flex flex-col gap-4" aria-label="Wynik">
      {pact.status === 'awaiting_result' && (
        <p className="rounded-lg border bg-card p-4 text-base">
          Termin minął. Ustalcie, jak to się skończyło.
        </p>
      )}
      {pact.proposals.length > 0 && (
        <ul className="flex flex-col gap-3">
          {pact.proposals.map((proposal) => (
            <Claim key={proposal.id} pact={pact} proposal={proposal} />
          ))}
        </ul>
      )}
      {participating && <ProposeForm key={pact.proposals.length} pact={pact} />}
    </section>
  )
}
