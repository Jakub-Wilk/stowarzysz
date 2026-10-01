import type { Tone } from '@/lib/api-types'

interface ToneClasses {
  /** Bordered, tinted panel. */
  panel: string
  /** Coloured text. */
  text: string
  /** Small filled badge. */
  chip: string
}

const TONES: Record<Tone, ToneClasses> = {
  positive: {
    panel: 'border-positive/60 bg-positive/10',
    text: 'text-positive',
    chip: 'bg-positive/15 text-positive',
  },
  neutral: {
    panel: 'border-neutral/50 bg-neutral/10',
    text: 'text-neutral',
    chip: 'bg-neutral/15 text-neutral',
  },
  negative: {
    panel: 'border-negative/60 bg-negative/10',
    text: 'text-negative',
    chip: 'bg-negative/15 text-negative',
  },
}

/** Green / gray / red styling for a vote outcome; no tone (e.g. nobody voted) reads as neutral. */
export function toneClasses(tone: Tone | null | undefined): ToneClasses {
  return TONES[tone ?? 'neutral']
}
