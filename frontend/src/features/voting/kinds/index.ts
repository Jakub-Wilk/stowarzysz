import type { PollKindUI } from '@/features/voting/kinds/types'
import {
  ScoreBallotInput,
  ScoreResultChip,
  ScoreResultDisplay,
} from '@/features/voting/kinds/score'

/**
 * UI for each vote kind, keyed by the backend's `kind`. A new kind (yes/no, options, ...) is one
 * entry here plus its components; list, detail and create screens don't change.
 */
const kinds: Record<string, PollKindUI> = {
  score: {
    BallotInput: ScoreBallotInput,
    ResultDisplay: ScoreResultDisplay,
    ResultChip: ScoreResultChip,
  },
}

export function getKindUI(kind: string): PollKindUI | null {
  return kinds[kind] ?? null
}
