// Hand-written for now; could be generated from /api/schema/ (openapi-typescript).

export interface TokenPair {
  access: string
  refresh: string
}

export interface Me {
  id: number
  username: string
  is_superuser: boolean
  /** Site-relative URL of the profile picture, if one is set. */
  avatar_url: string | null
}

/** Public, minimal account info for the login screen. */
export interface LoginUser {
  username: string
  avatar_url: string | null
}

export interface LoginPayload {
  username: string
  password: string
}

export interface ActivationValidateResult {
  username: string
}

export interface ActivationCompletePayload {
  token: string
  password: string
}

/** Account as seen by a superuser in the management UI. */
export interface ManagedUser {
  id: number
  username: string
  is_active: boolean
  is_superuser: boolean
  /** False until the user has set a password via an activation link. */
  has_password: boolean
  avatar_url: string | null
  voting: VotingStats
}

export interface VotingStats {
  votes_cast: number
  average_score: number | null
  veto_count: number
  veto_percent: number
}

export type ManagedUserPayload = Omit<ManagedUser, 'id' | 'has_password' | 'avatar_url' | 'voting'>

export interface ActivationLink {
  token: string
  url: string
  expires_at: string
}

// --- voting ---------------------------------------------------------------------------

export type PollStatus = 'open' | 'closed'
export type Tone = 'positive' | 'neutral' | 'negative'

/** Who someone is, as shown to other signed-in users. */
export interface UserBrief {
  id: number
  username: string
  avatar_url: string | null
}

/** Result of a finished vote; `tone` is common to all kinds, the rest depends on `kind`. */
export type PollResult = { tone: Tone | null } & Record<string, unknown>

/** A participant's ballot; its shape depends on the poll's kind. */
export type Ballot = Record<string, unknown>

export interface PollListItem {
  id: number
  title: string
  kind: string
  status: PollStatus
  creator: UserBrief
  created_at: string
  closed_at: string | null
  participant_count: number
  voted_count: number
  my: { participating: boolean; has_voted: boolean; vetoed: boolean }
  result: PollResult | null
}

export interface PollParticipant {
  user: UserBrief
  has_voted: boolean
  /** Only present once the vote has ended. */
  ballot?: Ballot | null
  vetoed?: boolean
}

export interface PollDetail extends PollListItem {
  config: Record<string, unknown>
  close_reason: 'auto' | 'creator' | null
  participants: PollParticipant[]
  my_ballot: Ballot | null
  can_vote: boolean
  can_close: boolean
}

export interface Paginated<T> {
  next: string | null
  previous: string | null
  results: T[]
}

export interface PollCreatePayload {
  title: string
  kind: string
  config: Record<string, unknown>
  participant_ids: number[]
}

export const REACTION_EMOJI = ['❤️', '🔥', '😭', '👎', '🤣'] as const
export type ReactionEmoji = (typeof REACTION_EMOJI)[number]

export interface ReactionEvent {
  poll_id: number
  emoji: ReactionEmoji
  user_id: number
}

export interface SantaEvent {
  id: number
  deadline: string
  /** PLN amounts, highest first; each giver gives one gift per tier. */
  gift_tiers: number[]
  participants: UserBrief[]
  is_participant: boolean
}

export interface SantaState {
  active: boolean
  event: SantaEvent | null
  /** Only ever set for the signed-in giver. */
  my_victim: UserBrief | null
}

export interface SantaStartPayload {
  participant_ids: number[]
  deadline: string
  gift_tiers: number[]
}

export interface SantaUpdatePayload {
  deadline?: string
  gift_tiers?: number[]
}

export interface SantaGift {
  id: number
  amount: number
  note: string
}

export interface SantaPairing {
  giver: UserBrief
  receiver: UserBrief
  gifts: SantaGift[]
}

export interface SantaHistoryEvent {
  id: number
  deadline: string
  ended_at: string
  gift_tiers: number[]
  pairings: SantaPairing[]
}
