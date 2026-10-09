import { Camera, StickyNote } from 'lucide-react'
import { useMemo, useRef, useState } from 'react'

import { Lightbox } from '@/components/Lightbox'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { MAX_PHOTOS, useDeleteAttachment } from '@/features/ledger/hooks'
import { AddPhotoTile, PhotoTile } from '@/features/ledger/PhotoTiles'
import { MutationError } from '@/features/pacts/MutationError'
import type { LedgerEntry } from '@/lib/api-types'

/** A photo picked in the form, shown from memory until it is uploaded with the entry. The
 * object URL is released once the thumbnail has loaded (a shown image stays shown). */
function StagedTile({ file, onRemove }: { file: File; onRemove: () => void }) {
  const url = useMemo(() => URL.createObjectURL(file), [file])
  return <PhotoTile src={url} onLoad={() => URL.revokeObjectURL(url)} onRemove={onRemove} />
}

/** The entry's photos while editing it: removing one takes effect at once, as on its page. */
function SavedTiles({ entry, myId }: { entry: LedgerEntry; myId: number }) {
  const remove = useDeleteAttachment(entry.id)
  const [shown, setShown] = useState<number | null>(null)
  return (
    <>
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
      <Lightbox
        images={entry.attachments.map((a) => ({ url: a.url }))}
        index={shown}
        onIndexChange={setShown}
      />
      {remove.error !== null && (
        <li className="col-span-4">
          <MutationError error={remove.error} />
        </li>
      )}
    </>
  )
}

interface ExtrasProps {
  note: string
  onNote: (note: string) => void
  /** Photos picked here, uploaded once the entry is saved. */
  photos: File[]
  onPhotos: (photos: File[]) => void
  /** When editing: the entry, whose saved photos are shown too. */
  entry?: LedgerEntry
  myId: number
}

/** The optional extras of an entry, a note and photos (a receipt): two buttons until used, so
 * the form stays short. */
export function Extras({ note, onNote, photos, onPhotos, entry, myId }: ExtrasProps) {
  const [noteOpen, setNoteOpen] = useState(note !== '')
  const picker = useRef<HTMLInputElement>(null)
  const saved = entry?.attachments.length ?? 0
  const room = MAX_PHOTOS - saved - photos.length
  const addPhotos = (files: File[]) => onPhotos([...photos, ...files.slice(0, Math.max(room, 0))])
  const showPhotos = saved + photos.length > 0

  return (
    <div className="flex flex-col gap-4">
      {(!noteOpen || !showPhotos) && (
        <div className="flex gap-2">
          {!noteOpen && (
            <Button
              type="button"
              variant="outline"
              className="flex-1"
              onClick={() => setNoteOpen(true)}
            >
              <StickyNote aria-hidden /> Notatka
            </Button>
          )}
          {!showPhotos && (
            <Button
              type="button"
              variant="outline"
              className="flex-1"
              onClick={() => picker.current?.click()}
            >
              <Camera aria-hidden /> Zdjęcia
            </Button>
          )}
        </div>
      )}
      <input
        ref={picker}
        type="file"
        accept="image/*"
        multiple
        className="hidden"
        aria-label="Dodaj zdjęcia"
        onChange={(e) => {
          addPhotos([...(e.target.files ?? [])])
          e.target.value = ''
        }}
      />

      {noteOpen && (
        <div className="flex flex-col gap-2">
          <Label htmlFor="entry-note">Notatka</Label>
          <Textarea
            id="entry-note"
            maxLength={500}
            autoFocus={note === ''}
            value={note}
            onChange={(e) => onNote(e.target.value)}
          />
        </div>
      )}

      {showPhotos && (
        <div className="flex flex-col gap-2">
          <span className="text-base font-medium">Zdjęcia</span>
          <ul className="grid grid-cols-4 gap-2">
            {entry && <SavedTiles entry={entry} myId={myId} />}
            {photos.map((file, i) => (
              <StagedTile
                key={`${file.name}-${file.lastModified}-${i}`}
                file={file}
                onRemove={() => onPhotos(photos.filter((_, j) => j !== i))}
              />
            ))}
            {room > 0 && <AddPhotoTile multiple onFiles={addPhotos} />}
          </ul>
        </div>
      )}
    </div>
  )
}
