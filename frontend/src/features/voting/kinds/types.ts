import type { ComponentType } from 'react'

import type { Ballot, PollDetail, PollResult } from '@/lib/api-types'

export interface BallotInputProps {
  /** The signed-in user's current ballot, if any. */
  value: Ballot | null
  pending: boolean
  onSubmit: (ballot: Ballot) => void
}

/** What the UI needs to know about one kind of vote; the generic screens never look inside. */
export interface PollKindUI {
  /** Control for casting (or changing) a ballot. */
  BallotInput: ComponentType<BallotInputProps>
  /** Full outcome of a finished vote, including who voted what. */
  ResultDisplay: ComponentType<{ poll: PollDetail }>
  /** Compact outcome for list rows. */
  ResultChip: ComponentType<{ result: PollResult }>
}
