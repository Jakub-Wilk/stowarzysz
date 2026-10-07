import type { PollKindUI } from '@/features/voting/kinds/types'
import {
  AvatarCreateForm,
  AvatarProposal,
  NicknameCreateForm,
  NicknameProposal,
  ProfileResultDisplay,
} from '@/features/voting/kinds/profile'
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
}

export function getKindUI(kind: string): PollKindUI | null {
  return kinds[kind] ?? null
}
