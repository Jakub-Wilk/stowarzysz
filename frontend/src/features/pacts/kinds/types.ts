import type { Tone } from '@/lib/api-types'
import type { PactDetail, PactKindKey, PactParticipant } from '@/lib/api-types'

/** One way a claim about the outcome can be worded; `result` is what the API receives. */
export interface OutcomeChoice {
  label: string
  result: Record<string, unknown>
  tone: Tone
}

/**
 * What the UI needs to know about one kind of pact; the generic screens never look inside, so a
 * new kind (mirrors `pacts/kinds.py`) is one file here plus an entry in `index.ts`.
 */
export interface PactKindUI {
  key: PactKindKey
  /** Short name used in lists and chips. */
  label: string
  /** One-line explanation shown when picking the kind. */
  blurb: string
  /** Where stakes come from: a wager per opponent, a shared pot, or none. */
  money: 'wager' | 'pot' | 'none'
  /** Participants pick one side of a question (stakes, if any, form a shared pot). */
  sided: boolean
  /** Each opponent is a separate wager against the host. */
  wagerBased: boolean
  /** The creator must set a deadline. */
  needsDue: boolean
  /** Anyone may ask to join (the participants decide). */
  joinable: boolean
  /** Nobody is invited: every other member judges it in a Sejmik vote. */
  judgedByEveryone: boolean
  /** The claims someone can make about how a wager (bets) or the whole pact ended. */
  outcomeChoices: (pact: PactDetail, wager: PactParticipant | null) => OutcomeChoice[]
  /** A claim as a sentence, e.g. "wygrywa alice". */
  describeResult: (
    result: Record<string, unknown>,
    pact: PactDetail,
    wager: PactParticipant | null,
  ) => string
}

export const VOID_CHOICE: OutcomeChoice = {
  label: 'Unieważnij (rozejdźmy się bez rozliczenia)',
  result: { void: true },
  tone: 'neutral',
}

export const DRAW_LABEL = 'Remis'
