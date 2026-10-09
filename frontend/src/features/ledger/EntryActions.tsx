import { Pencil } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'react-router'

import { Button } from '@/components/ui/button'
import { type EntryAction, useEntryAction } from '@/features/ledger/hooks'
import { kindOf } from '@/features/ledger/kinds'
import { MutationError } from '@/features/pacts/MutationError'
import { ConfirmDialog } from '@/features/voting/ConfirmDialog'
import type { LedgerEntry } from '@/lib/api-types'

/** The buttons the server says the signed-in user may press; nothing when there are none. */
export function EntryActions({ entry }: { entry: LedgerEntry }) {
  const action = useEntryAction()
  const [asking, setAsking] = useState(false)
  const kind = kindOf(entry)
  const labels = kind.actionLabels(entry)
  const { confirm, reject, cancel, edit } = entry.actions
  if (!confirm && !reject && !cancel && !edit) return null

  const run = (name: EntryAction) =>
    action.mutate({ id: entry.id, action: name }, { onSuccess: () => setAsking(false) })

  return (
    <div className="flex flex-col gap-2">
      {(confirm || reject) && (
        <div className="flex flex-col gap-2 min-[420px]:flex-row">
          {confirm && (
            <Button
              className="min-[420px]:flex-1"
              disabled={action.isPending}
              onClick={() => run('confirm')}
            >
              {labels.confirm}
            </Button>
          )}
          {reject && (
            <Button
              className="min-[420px]:flex-1"
              variant="outline"
              disabled={action.isPending}
              onClick={() => run('reject')}
            >
              {labels.reject}
            </Button>
          )}
        </div>
      )}
      {(edit || cancel) && (
        <div className="flex gap-2">
          {edit && (
            <Button
              className="flex-1"
              variant="outline"
              nativeButton={false}
              render={<Link to={`/ledger/${entry.id}/edit`} />}
            >
              <Pencil /> Edytuj
            </Button>
          )}
          {cancel && (
            <Button
              className="flex-1"
              variant="outline"
              disabled={action.isPending}
              onClick={() => (kind.cancelQuestion ? setAsking(true) : run('cancel'))}
            >
              {labels.cancel}
            </Button>
          )}
        </div>
      )}
      <MutationError error={action.error} />
      {kind.cancelQuestion && (
        <ConfirmDialog
          open={asking}
          onOpenChange={setAsking}
          title={labels.cancel}
          description={kind.cancelQuestion}
          confirmLabel={labels.cancel}
          pending={action.isPending}
          onConfirm={() => run('cancel')}
        />
      )}
    </div>
  )
}
