import { ImagePlus, Trash2 } from 'lucide-react'
import { useRef, useState } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAddAttachment, useDeleteAttachment } from '@/features/pacts/hooks'
import { MutationError } from '@/features/pacts/MutationError'
import type { PactDetail } from '@/lib/api-types'

/** Pictures that supplement the notes: open full size in a new tab, add if you take part. */
export function Attachments({ pact, myId }: { pact: PactDetail; myId: number | undefined }) {
  const add = useAddAttachment(pact.id)
  const remove = useDeleteAttachment(pact.id)
  const input = useRef<HTMLInputElement>(null)
  const [caption, setCaption] = useState('')

  const canAdd = pact.actions.can_attach
  if (pact.attachments.length === 0 && !canAdd) return null

  const upload = (file: File | undefined) => {
    if (!file) return
    add.mutate({ image: file, caption }, { onSuccess: () => setCaption('') })
    if (input.current) input.current.value = '' // allow picking the same file again
  }

  return (
    <section className="flex flex-col gap-3" aria-label="Zdjęcia">
      {pact.attachments.length > 0 && (
        <ul className="grid grid-cols-2 gap-3">
          {pact.attachments.map((a) => {
            const canDelete = a.uploaded_by.id === myId || pact.creator.id === myId
            return (
              <li key={a.id} className="relative overflow-hidden rounded-lg border bg-card">
                <a href={a.url} target="_blank" rel="noreferrer" className="block">
                  <img
                    src={a.url}
                    alt={a.caption || 'Zdjęcie do zakładu'}
                    loading="lazy"
                    className="aspect-square w-full object-cover"
                  />
                </a>
                {a.caption && <p className="px-3 py-2 text-sm">{a.caption}</p>}
                {canDelete && (
                  <Button
                    variant="secondary"
                    size="icon-sm"
                    className="absolute top-2 right-2"
                    aria-label="Usuń zdjęcie"
                    disabled={remove.isPending}
                    onClick={() => remove.mutate(a.id)}
                  >
                    <Trash2 />
                  </Button>
                )}
              </li>
            )
          })}
        </ul>
      )}
      {canAdd && (
        <div className="flex flex-col gap-2">
          <Input
            aria-label="Podpis zdjęcia"
            placeholder="Podpis (opcjonalnie)"
            maxLength={200}
            value={caption}
            onChange={(e) => setCaption(e.target.value)}
          />
          <input
            ref={input}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(e) => upload(e.target.files?.[0])}
          />
          <Button variant="outline" disabled={add.isPending} onClick={() => input.current?.click()}>
            <ImagePlus /> {add.isPending ? 'Wysyłanie…' : 'Dodaj zdjęcie'}
          </Button>
        </div>
      )}
      <MutationError error={add.error ?? remove.error} />
    </section>
  )
}
