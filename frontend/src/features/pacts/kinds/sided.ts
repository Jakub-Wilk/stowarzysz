import { DRAW_LABEL, type OutcomeChoice } from '@/features/pacts/kinds/types'
import type { PactDetail } from '@/lib/api-types'

/** Claims for kinds where everyone picked a side: one per side, or a draw. */
export function sideChoices(pact: PactDetail): OutcomeChoice[] {
  const sides = pact.config.sides ?? []
  return [
    ...sides.map((side): OutcomeChoice => ({
      label: `Wygrywa „${side}”`,
      result: { winner: side },
      tone: 'positive',
    })),
    { label: DRAW_LABEL, result: { winner: 'draw' }, tone: 'neutral' },
  ]
}

export function describeSideResult(result: Record<string, unknown>): string {
  if (result.void === true) return 'unieważnione'
  return result.winner === 'draw' ? 'remis' : `wygrywa „${String(result.winner)}”`
}
