import type { PollKindUI } from '@/features/voting/kinds/types'
import {
  AvatarCreateForm,
  AvatarProposal,
  NicknameCreateForm,
  NicknameProposal,
  ProfileResultDisplay,
} from '@/features/voting/kinds/profile'
import {
  RulingBallotInput,
  RulingProposal,
  RulingResultChip,
  RulingResultDisplay,
} from '@/features/voting/kinds/ruling'
import {
  ScoreBallotInput,
  ScoreResultChip,
  ScoreResultDisplay,
} from '@/features/voting/kinds/score'

/**
 * UI for each vote kind, keyed by the backend's `kind`. A new kind (yes/no, options, ...) is one
 * entry here plus its components; list, detail and create screens don't change.
 */
const score: PollKindUI = {
  BallotInput: ScoreBallotInput,
  ResultDisplay: ScoreResultDisplay,
  ResultChip: ScoreResultChip,
}

/** Votes to change another member's profile: scored like any vote, applied when they pass. */
const profileChange = { ...score, ResultDisplay: ProfileResultDisplay }

const kinds: Record<string, PollKindUI> = {
  score,
  nickname: { ...profileChange, Proposal: NicknameProposal, CreateForm: NicknameCreateForm },
  avatar: { ...profileChange, Proposal: AvatarProposal, CreateForm: AvatarCreateForm },
  /** Created by the server when the parties of a pact dispute a result; not in the create screen. */
  pact_ruling: {
    BallotInput: RulingBallotInput,
    ResultDisplay: RulingResultDisplay,
    ResultChip: RulingResultChip,
    Proposal: RulingProposal,
  },
}

export function getKindUI(kind: string): PollKindUI | null {
  return kinds[kind] ?? null
}
