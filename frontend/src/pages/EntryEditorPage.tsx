import { useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { Navigate, useNavigate, useParams } from 'react-router'

import { BackLink } from '@/components/layout/BackLink'
import { useMe } from '@/features/auth/hooks'
import { DebtForm } from '@/features/ledger/debt/DebtForm'
import { ExpenseForm } from '@/features/ledger/expense/ExpenseForm'
import { WORDING } from '@/features/ledger/expense/wording'
import {
  ledgerKey,
  uploadAttachment,
  useCreateEntry,
  useEntry,
  useLedgerMeta,
  useUpdateEntry,
} from '@/features/ledger/hooks'
import { kindOf } from '@/features/ledger/kinds'
import { ListSkeleton } from '@/features/ledger/LoadStates'
import { usePeople } from '@/features/voting/hooks'
import type { DebtInput, ExpenseInput, LedgerEntry, UserBrief } from '@/lib/api-types'

/** What can be added from the Rozliczenia tab, and edited later. */
const FORM_KINDS = ['expense', 'income', 'debt'] as const
type FormKind = (typeof FORM_KINDS)[number]
const isFormKind = (key: string | undefined): key is FormKind => FORM_KINDS.some((k) => k === key)

/** Shown on the entry's page when a photo didn't make it (the entry itself was saved). */
export interface EditorOutcome {
  photosFailed?: boolean
}

/**
 * A new entry (`/ledger/new/expense|income|debt`) or an existing one being edited
 * (`/ledger/:id/edit`). Photos picked in the form are uploaded once the entry is saved.
 */
export function EntryEditorPage() {
  const { id, kind: kindParam } = useParams()
  const editingId = id === undefined ? null : Number(id)
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { data: me } = useMe()
  const meta = useLedgerMeta()
  const members = usePeople()
  const existing = useEntry(editingId)
  const create = useCreateEntry()
  const update = useUpdateEntry(editingId ?? 0)
  const [uploading, setUploading] = useState<{ done: number; of: number } | null>(null)

  const entry = existing.data
  if (editingId !== null && (Number.isNaN(editingId) || existing.isError)) {
    return <Navigate to="/ledger" replace />
  }
  if (editingId === null && !isFormKind(kindParam)) {
    return <Navigate to="/ledger/new/expense" replace />
  }
  if (entry && (!isFormKind(entry.kind) || !entry.actions.edit)) {
    return <Navigate to={`/ledger/${entry.id}`} replace />
  }
  const kind: FormKind = entry && isFormKind(entry.kind) ? entry.kind : (kindParam as FormKind)
  const back = editingId === null ? '/ledger' : `/ledger/${editingId}`
  const loading = !me || !meta.data || !members.data || (editingId !== null && !entry)

  // members, plus anyone the edited entry mentions who has left since
  const people: UserBrief[] = [...(members.data ?? [])]
  for (const user of [
    ...(entry?.breakdown ?? []).map((row) => row.user),
    ...(entry?.obligations ?? []).flatMap((o) => [o.debtor, o.creditor]),
  ]) {
    if (!people.some((p) => p.id === user.id)) people.push(user)
  }

  /** Upload the photos one by one (a failed one doesn't stop the rest), then show the entry. */
  const finish = async (saved: LedgerEntry, photos: File[]) => {
    let failed = false
    for (const [i, file] of photos.entries()) {
      setUploading({ done: i, of: photos.length })
      try {
        await uploadAttachment(saved.id, file)
      } catch {
        failed = true
      }
    }
    if (photos.length) await queryClient.invalidateQueries({ queryKey: ledgerKey })
    const outcome: EditorOutcome = { photosFailed: failed }
    navigate(`/ledger/${saved.id}`, { replace: true, state: outcome })
  }
  const submit = (input: ExpenseInput | DebtInput, photos: File[]) => {
    const done = { onSuccess: (saved: LedgerEntry) => void finish(saved, photos) }
    if (entry) update.mutate({ input, version: entry.version }, done)
    else create.mutate(input, done)
  }

  const shared = {
    meta: meta.data,
    people,
    myId: me?.id ?? 0,
    pending: create.isPending || update.isPending || uploading !== null,
    pendingLabel: uploading ? `Wysyłanie zdjęć ${uploading.done + 1}/${uploading.of}…` : undefined,
    error: create.error ?? update.error,
    onSubmit: submit,
  }
  const heading =
    kind === 'debt'
      ? entry
        ? 'Edycja długu'
        : 'Nowy dług'
      : entry
        ? WORDING[kind].editHeading
        : WORDING[kind].newHeading

  return (
    <>
      <BackLink to={back}>{entry ? kindOf(entry).label : 'Rozliczenia'}</BackLink>
      <h2 className="mb-6 text-2xl font-semibold">{heading}</h2>
      {loading || !shared.meta ? (
        <ListSkeleton label="Ładowanie formularza" />
      ) : kind === 'debt' ? (
        <DebtForm
          {...shared}
          meta={shared.meta}
          entry={entry?.kind === 'debt' ? entry : undefined}
        />
      ) : (
        <ExpenseForm
          {...shared}
          meta={shared.meta}
          kind={kind}
          onKind={entry ? undefined : (next) => navigate(`/ledger/new/${next}`, { replace: true })}
          entry={entry?.kind === 'expense' || entry?.kind === 'income' ? entry : undefined}
        />
      )}
    </>
  )
}
