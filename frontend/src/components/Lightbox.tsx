import { ChevronLeft, ChevronRight } from 'lucide-react'
import { useEffect } from 'react'

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog'

export interface LightboxImage {
  url: string
  caption?: string
}

interface LightboxProps {
  images: LightboxImage[]
  /** Index of the picture on show; null keeps it closed. */
  index: number | null
  onIndexChange: (index: number | null) => void
}

/** The picture at full size over a dimmed page. Arrow keys and the buttons flip through. */
export function Lightbox({ images, index, onIndexChange }: LightboxProps) {
  const current = index !== null && images[index] !== undefined ? index : null
  const open = current !== null
  const image = current !== null ? images[current] : null
  const count = images.length

  useEffect(() => {
    if (current === null) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'ArrowLeft') onIndexChange((current + count - 1) % count)
      else if (e.key === 'ArrowRight') onIndexChange((current + 1) % count)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [current, count, onIndexChange])

  return (
    <Dialog open={open} onOpenChange={(next) => !next && onIndexChange(null)}>
      <DialogContent className="max-w-[calc(100%-1rem)] gap-2 p-2 sm:max-w-4xl">
        <DialogTitle className="sr-only">
          Zdjęcie {current !== null ? `${current + 1} z ${count}` : ''}
        </DialogTitle>
        <DialogDescription className="sr-only">{image?.caption ?? 'Zdjęcie'}</DialogDescription>
        {image && (
          <>
            <img
              src={image.url}
              alt={image.caption || 'Zdjęcie'}
              className="max-h-[80svh] w-full rounded-lg object-contain"
            />
            {image.caption && <p className="px-2 text-center text-base">{image.caption}</p>}
            {count > 1 && (
              <>
                <Button
                  variant="secondary"
                  size="icon"
                  className="absolute top-1/2 left-3 -translate-y-1/2"
                  aria-label="Poprzednie zdjęcie"
                  onClick={() => onIndexChange(((current ?? 0) + count - 1) % count)}
                >
                  <ChevronLeft />
                </Button>
                <Button
                  variant="secondary"
                  size="icon"
                  className="absolute top-1/2 right-3 -translate-y-1/2"
                  aria-label="Następne zdjęcie"
                  onClick={() => onIndexChange(((current ?? 0) + 1) % count)}
                >
                  <ChevronRight />
                </Button>
              </>
            )}
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
