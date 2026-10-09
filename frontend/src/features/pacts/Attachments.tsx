import { ImagePlus, Trash2 } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'

import { Lightbox } from '@/components/Lightbox'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { useAddAttachment, useDeleteAttachment } from '@/features/pacts/hooks'
import { MutationError } from '@/features/pacts/MutationError'
import type { PactDetail } from '@/lib/api-types'

/** Preview of a freshly chosen picture, with a caption, before it is uploaded. */
function AddDialog({
  pact,
  file,
  onClose,
}: {
  pact: PactDetail
  file: File | null
  onClose: () => void
}) {
  const add = useAddAttachment(pact.id)
  const [caption, setCaption] = useState('')
  const preview = useMemo(() => (file ? URL.createObjectURL(file) : null), [file])
  useEffect(() => () => (preview ? URL.revokeObjectURL(preview) : undefined), [preview])

  return (
    <Dialog
      open={file !== null}
      onOpenChange={(open) => {
        if (!open) {
          setCaption('')
          add.reset()
          onClose()
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Dodaj zdjęcie</DialogTitle>
          <DialogDescription>Podpis jest opcjonalny.</DialogDescription>
        </DialogHeader>
        {preview && (
          <img src={preview} alt="Podgląd" className="max-h-64 w-full rounded-lg object-contain" />
        )}
        <Input
          aria-label="Podpis zdjęcia"
          placeholder="Podpis (opcjonalnie)"
          maxLength={200}
          value={caption}
          onChange={(e) => setCaption(e.target.value)}
        />
        <MutationError error={add.error} />
        <DialogFooter>
          <Button
            disabled={add.isPending || !file}
            onClick={() =>
              file &&
              add.mutate(
                { image: file, caption },
                {
                  onSuccess: () => {
                    setCaption('')
                    onClose()
                  },
                },
              )
            }
          >
            {add.isPending ? 'Wysyłanie…' : 'Dodaj'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

/** Pictures that supplement the notes: small tiles that open in a lightbox, plus an add tile. */
export function Attachments({ pact, myId }: { pact: PactDetail; myId: number | undefined }) {
  const remove = useDeleteAttachment(pact.id)
  const input = useRef<HTMLInputElement>(null)
  const [chosen, setChosen] = useState<File | null>(null)
  const [shown, setShown] = useState<number | null>(null)

  const canAdd = pact.actions.can_attach
  if (pact.attachments.length === 0 && !canAdd) return null

  return (
    <section className="flex flex-col gap-3" aria-label="Zdjęcia">
      <ul className="grid grid-cols-4 gap-2">
        {pact.attachments.map((a, i) => {
          const canDelete = a.uploaded_by.id === myId || pact.creator.id === myId
          return (
            <li
              key={a.id}
              className="relative aspect-square overflow-hidden rounded-lg border bg-card"
            >
              <button
                type="button"
                className="block size-full"
                aria-label={a.caption ? `Powiększ: ${a.caption}` : 'Powiększ zdjęcie'}
                onClick={() => setShown(i)}
              >
                <img
                  src={a.url}
                  alt={a.caption || 'Zdjęcie do zakładu'}
                  loading="lazy"
                  className="size-full object-cover"
                />
              </button>
              {a.caption && (
                <span className="pointer-events-none absolute inset-x-0 bottom-0 truncate bg-background/80 px-1.5 py-0.5 text-xs">
                  {a.caption}
                </span>
              )}
              {canDelete && (
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
          )
        })}
        {canAdd && (
          <li className="aspect-square">
            <input
              ref={input}
              type="file"
              accept="image/*"
              className="hidden"
              onChange={(e) => {
                setChosen(e.target.files?.[0] ?? null)
                e.target.value = '' // allow picking the same file again
              }}
            />
            <button
              type="button"
              className="flex size-full flex-col items-center justify-center gap-1 rounded-lg border border-dashed text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:border-ring focus-visible:outline-none"
              onClick={() => input.current?.click()}
            >
              <ImagePlus className="size-6" aria-hidden />
              Dodaj zdjęcie
            </button>
          </li>
        )}
      </ul>
      <MutationError error={remove.error} />
      <AddDialog pact={pact} file={chosen} onClose={() => setChosen(null)} />
      <Lightbox
        images={pact.attachments.map((a) => ({ url: a.url, caption: a.caption }))}
        index={shown}
        onIndexChange={setShown}
      />
    </section>
  )
}
