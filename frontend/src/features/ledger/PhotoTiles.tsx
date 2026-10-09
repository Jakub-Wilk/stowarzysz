import { ImagePlus, X } from 'lucide-react'
import { useRef } from 'react'

import { Button } from '@/components/ui/button'

/** A square photo thumbnail that opens larger, with an optional remove button. */
export function PhotoTile({
  src,
  onOpen,
  onRemove,
  onLoad,
  disabled,
}: {
  src: string
  onLoad?: () => void
  onOpen?: () => void
  onRemove?: () => void
  disabled?: boolean
}) {
  return (
    <li className="relative aspect-square overflow-hidden rounded-lg border bg-card">
      <button
        type="button"
        className="block size-full"
        aria-label="Powiększ zdjęcie"
        disabled={!onOpen}
        onClick={onOpen}
      >
        <img
          src={src}
          alt="Zdjęcie do wpisu"
          loading="lazy"
          className="size-full object-cover"
          onLoad={onLoad}
        />
      </button>
      {onRemove && (
        <Button
          type="button"
          variant="secondary"
          size="icon-xs"
          className="absolute top-1 right-1"
          aria-label="Usuń zdjęcie"
          disabled={disabled}
          onClick={onRemove}
        >
          <X />
        </Button>
      )}
    </li>
  )
}

/** The dashed tile that adds photos (phones offer the camera too). */
export function AddPhotoTile({
  onFiles,
  multiple,
  pending,
}: {
  onFiles: (files: File[]) => void
  multiple?: boolean
  pending?: boolean
}) {
  const input = useRef<HTMLInputElement>(null)
  return (
    <li className="aspect-square">
      <input
        ref={input}
        type="file"
        accept="image/*"
        multiple={multiple}
        className="hidden"
        onChange={(e) => {
          const files = [...(e.target.files ?? [])]
          if (files.length) onFiles(files)
          e.target.value = '' // allow picking the same file again
        }}
      />
      <button
        type="button"
        disabled={pending}
        className="flex size-full flex-col items-center justify-center gap-1 rounded-lg border border-dashed text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:border-ring focus-visible:outline-none"
        onClick={() => input.current?.click()}
      >
        <ImagePlus className="size-6" aria-hidden />
        {pending ? 'Wysyłanie…' : 'Dodaj zdjęcie'}
      </button>
    </li>
  )
}
