import { ImagePlus, Trash2 } from 'lucide-react'
import { useRef, useState } from 'react'

import { Lightbox } from '@/components/Lightbox'
import { Button } from '@/components/ui/button'
import { useAddAttachment, useDeleteAttachment } from '@/features/ledger/hooks'
import { MutationError } from '@/features/pacts/MutationError'
import type { LedgerEntry } from '@/lib/api-types'

/** Photos on an entry (a receipt, proof of a transfer): tiles that open in a lightbox. */
export function EntryAttachments({
  entry,
  myId,
}: {
  entry: LedgerEntry
  myId: number | undefined
}) {
  const add = useAddAttachment(entry.id)
  const remove = useDeleteAttachment(entry.id)
  const input = useRef<HTMLInputElement>(null)
  const [shown, setShown] = useState<number | null>(null)

  const canAdd = entry.actions.attach && entry.attachments.length < 10
  if (entry.attachments.length === 0 && !canAdd) return null

  return (
    <section className="flex flex-col gap-3" aria-label="Zdjęcia">
      <ul className="grid grid-cols-4 gap-2">
        {entry.attachments.map((a, i) => (
          <li
            key={a.id}
            className="relative aspect-square overflow-hidden rounded-lg border bg-card"
          >
            <button
              type="button"
              className="block size-full"
              aria-label="Powiększ zdjęcie"
              onClick={() => setShown(i)}
            >
              <img
                src={a.url}
                alt="Zdjęcie do wpisu"
                loading="lazy"
                className="size-full object-cover"
              />
            </button>
            {(a.uploaded_by.id === myId || entry.created_by?.id === myId) && (
              <Button
                variant="secondary"
                size="icon-xs"
                className="absolute top-1 right-1"
                aria-label="Usuń zdjęcie"
                disabled={remove.isPending}
                onClick={() => remove.mutate(a.id)}
              >
                <Trash2 />
              </Button>
            )}
          </li>
        ))}
        {canAdd && (
          <li className="aspect-square">
            <input
              ref={input}
              type="file"
              accept="image/*"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0]
                if (file) add.mutate(file)
                e.target.value = '' // allow picking the same file again
              }}
            />
            <button
              type="button"
              disabled={add.isPending}
              className="flex size-full flex-col items-center justify-center gap-1 rounded-lg border border-dashed text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:border-ring focus-visible:outline-none"
              onClick={() => input.current?.click()}
            >
              <ImagePlus className="size-6" aria-hidden />
              {add.isPending ? 'Wysyłanie…' : 'Dodaj zdjęcie'}
            </button>
          </li>
        )}
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
