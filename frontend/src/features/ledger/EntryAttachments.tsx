import { useState } from 'react'

import { Lightbox } from '@/components/Lightbox'
import { MAX_PHOTOS, useAddAttachment, useDeleteAttachment } from '@/features/ledger/hooks'
import { AddPhotoTile, PhotoTile } from '@/features/ledger/PhotoTiles'
import { MutationError } from '@/features/pacts/MutationError'
import type { LedgerEntry } from '@/lib/api-types'

/** Photos on an entry (a receipt, proof of a transfer): tiles that open in a lightbox. Adding
 * or removing one takes effect at once. */
export function EntryAttachments({
  entry,
  myId,
}: {
  entry: LedgerEntry
  myId: number | undefined
}) {
  const add = useAddAttachment(entry.id)
  const remove = useDeleteAttachment(entry.id)
  const [shown, setShown] = useState<number | null>(null)

  const canAdd = entry.actions.attach && entry.attachments.length < MAX_PHOTOS
  if (entry.attachments.length === 0 && !canAdd) return null

  return (
    <section className="flex flex-col gap-3" aria-label="Zdjęcia">
      <ul className="grid grid-cols-4 gap-2">
        {entry.attachments.map((a, i) => (
          <PhotoTile
            key={a.id}
            src={a.url}
            onOpen={() => setShown(i)}
            onRemove={
              a.uploaded_by.id === myId || entry.created_by?.id === myId
                ? () => remove.mutate(a.id)
                : undefined
            }
            disabled={remove.isPending}
          />
        ))}
        {canAdd && <AddPhotoTile pending={add.isPending} onFiles={([file]) => add.mutate(file)} />}
      </ul>
      <MutationError error={add.error ?? remove.error} />
      <Lightbox
        images={entry.attachments.map((a) => ({ url: a.url }))}
        index={shown}
        onIndexChange={setShown}
      />
    </section>
  )
}
