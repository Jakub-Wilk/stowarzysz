import { Navigate, useLocation, useParams } from 'react-router'

import { BackLink } from '@/components/layout/BackLink'
import { useMe } from '@/features/auth/hooks'
import { EntryActions } from '@/features/ledger/EntryActions'
import { EntryAttachments } from '@/features/ledger/EntryAttachments'
import { EntryStatusChip } from '@/features/ledger/EntryRow'
import {
  formatDay,
  formatImpact,
  impactOf,
  impactTone,
  useCategory,
  useMoney,
} from '@/features/ledger/format'
import { useEntry } from '@/features/ledger/hooks'
import { kindOf } from '@/features/ledger/kinds'
import { LoadError } from '@/features/ledger/LoadStates'
import { KindChip } from '@/features/pacts/StatusChip'
import { toneClasses } from '@/features/voting/tone'
import { ApiError } from '@/lib/api'
import type { LedgerEntry } from '@/lib/api-types'
import type { EditorOutcome } from '@/pages/EntryEditorPage'
import { cn } from '@/lib/utils'

function EntryView({
  entry,
  myId,
  photosFailed,
}: {
  entry: LedgerEntry
  myId: number | undefined
  photosFailed: boolean
}) {
  const kind = kindOf(entry)
  const money = useMoney()
  const category = useCategory()(entry)
  const impact = impactOf(entry, myId)
  const mine = impact ? formatImpact(impact) : null
  const headline = money.headline(entry)
  const edited = entry.version > 1 && entry.status === 'confirmed' && entry.kind !== 'payment'

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-2">
        <h2 className="text-3xl font-semibold break-words">{entry.title}</h2>
        <div className="flex flex-wrap items-center gap-2 text-base text-muted-foreground">
          <KindChip label={kind.label} />
          <EntryStatusChip entry={entry} />
          <span>{formatDay(entry.occurred_on)}</span>
          {category && (
            <span className="whitespace-nowrap">
              <span aria-hidden>{category.emoji}</span> {category.label}
            </span>
          )}
        </div>
      </header>

      {(headline || mine) && (
        <div className="flex items-baseline justify-between gap-4">
          {headline && <span className="text-4xl font-bold tabular-nums">{headline}</span>}
          {impact && mine && (
            <span
              className={cn(
                'text-lg tabular-nums',
                toneClasses(kind.settles ? 'neutral' : impactTone(impact)).text,
              )}
            >
              Ty: {mine}
            </span>
          )}
        </div>
      )}

      <kind.Detail entry={entry} myId={myId} />

      {entry.note && <p className="text-base whitespace-pre-wrap">{entry.note}</p>}

      {photosFailed && (
        <p role="alert" className="text-base text-destructive">
          Nie udało się wysłać części zdjęć. Dodaj je tutaj.
        </p>
      )}
      <EntryAttachments entry={entry} myId={myId} />
      <EntryActions entry={entry} />

      <p className="text-sm text-muted-foreground">
        {entry.created_by ? `Dodał(a) ${entry.created_by.username}` : 'Zapisane automatycznie'}
        {edited && ', później edytowane'}
      </p>
    </div>
  )
}

export function LedgerEntryPage() {
  const { id } = useParams()
  const entryId = Number(id)
  const valid = Number.isInteger(entryId)
  const { data: me } = useMe()
  const { data: entry, error, isPending, refetch } = useEntry(valid ? entryId : null)
  const outcome = useLocation().state as EditorOutcome | null

  if (!valid || (error instanceof ApiError && error.status === 404)) {
    return <Navigate to="/ledger" replace />
  }

  return (
    <>
      <BackLink to="/ledger">Rozliczenia</BackLink>
      {isPending && <span className="text-sm text-muted-foreground">Ładowanie…</span>}
      {error && !isPending && <LoadError what="wpisu" onRetry={() => void refetch()} />}
      {entry && (
        <EntryView entry={entry} myId={me?.id} photosFailed={Boolean(outcome?.photosFailed)} />
      )}
    </>
  )
}
