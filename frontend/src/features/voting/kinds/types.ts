import type { ComponentType } from 'react'

import type { Ballot, PollCreatePayload, PollDetail, PollResult, UserBrief } from '@/lib/api-types'

export interface BallotInputProps {
  /** The signed-in user's current ballot, if any. */
  value: Ballot | null
  pending: boolean
  onSubmit: (ballot: Ballot) => void
}

export interface CreateFormProps {
  /** Everyone except the signed-in user. */
  people: UserBrief[]
  pending: boolean
  /** Failed create request, if any. */
  error: unknown
  onSubmit: (payload: PollCreatePayload) => void
}

/** What the UI needs to know about one kind of vote; the generic screens never look inside. */
export interface PollKindUI {
  /** Control for casting (or changing) a ballot. */
  BallotInput: ComponentType<BallotInputProps>
  /** Full outcome of a finished vote, including who voted what. */
  ResultDisplay: ComponentType<{ poll: PollDetail }>
  /** Compact outcome for list rows. */
  ResultChip: ComponentType<{ result: PollResult }>
  /** What is being voted on, shown above the ballot and the result (kinds with a proposal). */
  Proposal?: ComponentType<{ poll: PollDetail }>
  /** Own creation form, for kinds that need more than a title and participants. */
  CreateForm?: ComponentType<CreateFormProps>
}
