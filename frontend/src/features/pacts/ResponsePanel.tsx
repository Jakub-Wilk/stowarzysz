import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { ConfirmDialog } from '@/features/voting/ConfirmDialog'
import { formatStake } from '@/features/pacts/format'
import {
  useCancelPact,
  useRequestJoin,
  useRespond,
  useWithdrawRequest,
} from '@/features/pacts/hooks'
import { getPactKind } from '@/features/pacts/kinds'
import { MutationError } from '@/features/pacts/MutationError'
import { TermsFields } from '@/features/pacts/TermsFields'
import { buildTerms, emptyTerms, type TermsState } from '@/features/pacts/terms'
import type { PactDetail } from '@/lib/api-types'

/** Starting values for the terms form: what the creator already filled in for me. */
function initialTerms(pact: PactDetail): TermsState {
  const mine = pact.participants.find((p) => p.id === pact.my.participant_id)
  if (!mine) return emptyTerms
  return {
    side: mine.side,
    amount: mine.stake_amount ? String(mine.stake_amount / 100).replace('.', ',') : '',
    note: mine.stake_note,
  }
}

function InvitePanel({ pact }: { pact: PactDetail }) {
  const kind = getPactKind(pact.kind)
  const respond = useRespond(pact.id)
  const [terms, setTerms] = useState(() => initialTerms(pact))
  const [localError, setLocalError] = useState<string | null>(null)
  const mine = pact.participants.find((p) => p.id === pact.my.participant_id)
  const asksTerms = !kind.wagerBased && (kind.sided || kind.money !== 'none')

  const accept = () => {
    setLocalError(null)
    if (!asksTerms) return respond.mutate({ accept: true })
    const built = buildTerms(kind, terms)
    if (built.error !== null) return setLocalError(built.error)
    respond.mutate({ accept: true, ...built.terms })
  }

  return (
    <section className="flex flex-col gap-4 rounded-lg border border-primary/60 bg-card p-4">
      <h3 className="text-lg font-semibold">{pact.creator.username} zaprasza Cię</h3>
      {kind.wagerBased && mine && (
        <p className="text-base">
          Twoja stawka: <strong>{formatStake(mine) ?? 'brak'}</strong>. Jeśli{' '}
          {pact.creator.username} wygra, płacisz tę stawkę; jeśli przegra, płaci ją Tobie.
        </p>
      )}
      {kind.key === 'group_bet' && (
        <p className="text-base">
          Wybierz stronę i stawkę. Zwycięzcy dzielą stawki przegranych proporcjonalnie, nikt nie
          traci więcej niż swoją stawkę.
        </p>
      )}
      {asksTerms && (
        <TermsFields
          kind={kind}
          sides={pact.config.sides ?? []}
          state={terms}
          onChange={setTerms}
          idPrefix="respond"
          disabled={respond.isPending}
        />
      )}
      {localError && (
        <span role="alert" className="text-base text-destructive">
          {localError}
        </span>
      )}
      <div className="flex gap-2">
        <Button className="flex-1" disabled={respond.isPending} onClick={accept}>
          Przyjmuję
        </Button>
        <Button
          className="flex-1"
          variant="outline"
          disabled={respond.isPending}
          onClick={() => respond.mutate({ accept: false })}
        >
          Odmawiam
        </Button>
      </div>
      <MutationError error={respond.error} />
    </section>
  )
}

function JoinPanel({ pact }: { pact: PactDetail }) {
  const kind = getPactKind(pact.kind)
  const join = useRequestJoin(pact.id)
  const [open, setOpen] = useState(false)
  const [terms, setTerms] = useState<TermsState>(emptyTerms)
  const [localError, setLocalError] = useState<string | null>(null)

  const submit = () => {
    setLocalError(null)
    const built = buildTerms(kind, terms)
    if (built.error !== null) return setLocalError(built.error)
    join.mutate(built.terms, { onSuccess: () => setOpen(false) })
  }

  if (!open) {
    return (
      <Button className="w-full" onClick={() => setOpen(true)}>
        Poproś o dołączenie
      </Button>
    )
  }
  return (
    <section className="flex flex-col gap-4 rounded-lg border bg-card p-4">
      <h3 className="text-lg font-semibold">Dołącz do zakładu</h3>
      {kind.wagerBased && (
        <p className="text-base">
          Stawiasz przeciwko {pact.creator.username}. Podaj własną stawkę, a autor zdecyduje, czy ją
          przyjmie.
        </p>
      )}
      <TermsFields
        kind={kind}
        sides={pact.config.sides ?? []}
        state={terms}
        onChange={setTerms}
        idPrefix="join"
        disabled={join.isPending}
      />
      {localError && (
        <span role="alert" className="text-base text-destructive">
          {localError}
        </span>
      )}
      <div className="flex gap-2">
        <Button className="flex-1" disabled={join.isPending} onClick={submit}>
          Wyślij prośbę
        </Button>
        <Button variant="outline" onClick={() => setOpen(false)}>
          Anuluj
        </Button>
      </div>
      <MutationError error={join.error} />
    </section>
  )
}

/** Everything the signed-in user can do about their own place in the pact. */
export function ResponsePanel({ pact }: { pact: PactDetail }) {
  const withdraw = useWithdrawRequest(pact.id)
  const cancel = useCancelPact(pact.id)
  const [confirmCancel, setConfirmCancel] = useState(false)
  const { actions } = pact
  const canCancel = pact.my.role === 'host' && pact.status === 'proposed'

  return (
    <>
      {actions.can_respond && <InvitePanel pact={pact} />}
      {actions.can_request_join && <JoinPanel pact={pact} />}
      {actions.can_withdraw_request && (
        <section className="flex flex-col gap-2 rounded-lg border bg-card p-4">
          <p className="text-base">Twoja prośba o dołączenie czeka na decyzję.</p>
          <Button variant="outline" disabled={withdraw.isPending} onClick={() => withdraw.mutate()}>
            Wycofaj prośbę
          </Button>
          <MutationError error={withdraw.error} />
        </section>
      )}
      {canCancel && (
        <>
          <Button variant="outline" className="w-full" onClick={() => setConfirmCancel(true)}>
            Anuluj zakład
          </Button>
          <ConfirmDialog
            open={confirmCancel}
            onOpenChange={setConfirmCancel}
            title="Anulować zakład?"
            description="Zakład jeszcze się nie rozpoczął. Zaproszeni dostaną powiadomienie, a zakładu nie będzie można wznowić."
            confirmLabel="Anuluj zakład"
            pending={cancel.isPending}
            onConfirm={() => cancel.mutate(undefined, { onSettled: () => setConfirmCancel(false) })}
          />
          <MutationError error={cancel.error} />
        </>
      )}
    </>
  )
}
