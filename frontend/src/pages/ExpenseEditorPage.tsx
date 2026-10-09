import { Navigate, useNavigate, useParams } from 'react-router'

import { BackLink } from '@/components/layout/BackLink'
import { useMe } from '@/features/auth/hooks'
import { ExpenseForm } from '@/features/ledger/expense/ExpenseForm'
import { useCreateEntry, useEntry, useLedgerMeta, useUpdateExpense } from '@/features/ledger/hooks'
import { ListSkeleton } from '@/features/ledger/LoadStates'
import { usePeople } from '@/features/voting/hooks'
import type { ExpenseInput, LedgerEntry, UserBrief } from '@/lib/api-types'

/** A new expense (`/ledger/new`) or an existing one being edited (`/ledger/:id/edit`). */
export function ExpenseEditorPage() {
  const { id } = useParams()
  const editingId = id === undefined ? null : Number(id)
  const navigate = useNavigate()
  const { data: me } = useMe()
  const meta = useLedgerMeta()
  const members = usePeople()
  const existing = useEntry(editingId)
  const create = useCreateEntry()
  const update = useUpdateExpense(editingId ?? 0)

  const entry = existing.data
  if (editingId !== null && (Number.isNaN(editingId) || existing.isError)) {
    return <Navigate to="/ledger" replace />
  }
  if (entry && (entry.kind !== 'expense' || !entry.actions.edit)) {
    return <Navigate to={`/ledger/${entry.id}`} replace />
  }
  const back = editingId === null ? '/ledger' : `/ledger/${editingId}`
  const loading = !me || !meta.data || !members.data || (editingId !== null && !entry)

  // members, plus anyone the edited expense mentions who has left since
  const people: UserBrief[] = [...(members.data ?? [])]
  for (const row of entry?.kind === 'expense' ? entry.breakdown : []) {
    if (!people.some((p) => p.id === row.user.id)) people.push(row.user)
  }

  const saved = (saved: LedgerEntry) => navigate(`/ledger/${saved.id}`, { replace: true })
  const submit = (input: ExpenseInput) =>
    entry
      ? update.mutate({ input, version: entry.version }, { onSuccess: saved })
      : create.mutate(input, { onSuccess: saved })

  return (
    <>
      <BackLink to={back}>{entry ? 'Wydatek' : 'Rozliczenia'}</BackLink>
      <h2 className="mb-6 text-2xl font-semibold">{entry ? 'Edycja wydatku' : 'Nowy wydatek'}</h2>
      {loading || !me || !meta.data ? (
        <ListSkeleton label="Ładowanie formularza" />
      ) : (
        <ExpenseForm
          meta={meta.data}
          people={people}
          myId={me.id}
          entry={entry?.kind === 'expense' ? entry : undefined}
          pending={create.isPending || update.isPending}
          error={create.error ?? update.error}
          onSubmit={submit}
        />
      )}
    </>
  )
}
