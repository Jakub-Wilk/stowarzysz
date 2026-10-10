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
  /** ISO date (YYYY-MM-DD), `null` when unknown. */
  birthday: string | null
  voting: VotingStats
  pacts: PactStats
}

export interface VotingStats {
  votes_cast: number
  average_score: number | null
  veto_count: number
  veto_percent: number
}

export type ManagedUserPayload = Omit<
  ManagedUser,
  'id' | 'has_password' | 'avatar_url' | 'voting' | 'pacts'
>

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
  close_reason: 'auto' | 'creator' | 'expired' | null
  participants: PollParticipant[]
  my_ballot: Ballot | null
  can_vote: boolean
  can_close: boolean
  can_veto: boolean
  /** Picture proposed by a profile-picture vote (kept after it closes). */
  proposed_avatar_url: string | null
  /** The picture an approved profile-picture vote replaced; null if none (or an older vote). */
  previous_avatar_url: string | null
}

export interface Paginated<T> {
  next: string | null
  previous: string | null
  results: T[]
}

export interface PollCreatePayload {
  /** Omitted for votes whose title the server generates (nickname, profile picture). */
  title?: string
  kind: string
  config: Record<string, unknown>
  /** Omitted when everyone takes part. */
  participant_ids?: number[]
  /** Proposed profile picture (sent as multipart). */
  image?: File
}

export const REACTION_EMOJI = ['❤️', '🔥', '😭', '👎', '🤣'] as const
export type ReactionEmoji = (typeof REACTION_EMOJI)[number]

export interface ReactionEvent {
  poll_id: number
  emoji: ReactionEmoji
  user_id: number
}

/** `single`: one victim gets every tier. `per_tier`: a different victim for each tier. */
export type SantaMode = 'single' | 'per_tier'

export interface SantaEvent {
  id: number
  mode: SantaMode
  deadline: string
  /** PLN amounts, highest first; each giver gives one gift per tier. */
  gift_tiers: number[]
  participants: UserBrief[]
  is_participant: boolean
}

export interface SantaState {
  active: boolean
  event: SantaEvent | null
  /** Only ever set for the signed-in giver, in `single` mode. */
  my_victim: UserBrief | null
  /** Only ever set for the signed-in giver, in `per_tier` mode; highest amount first. */
  my_tier_victims: SantaTierVictim[]
  /** Help the signed-in giver asked their victims for. */
  my_help_requests: SantaGiverHelp[]
  /** Anonymous requests for gift ideas about the signed-in user; never says who asked. */
  help_requests_for_me: SantaReceiverHelp[]
}

/** A help request as its giver sees it. `amount` is null in `single` mode (every tier). */
export interface SantaGiverHelp {
  victim_id: number
  amount: number | null
  /** Waiting for the victim to answer. */
  pending: boolean
  /** The victim's latest ideas (kept while a repeated request is pending). */
  ideas: string[]
}

/** A help request as its victim sees it: which gift, never who asked. */
export interface SantaReceiverHelp {
  id: number
  amount: number | null
  pending: boolean
  ideas: string[]
}

export interface SantaTierVictim {
  amount: number
  victim: UserBrief
}

export interface SantaStartPayload {
  participant_ids: number[]
  deadline: string
  gift_tiers: number[]
  mode: SantaMode
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
  id: number
  giver: UserBrief
  receiver: UserBrief
  gifts: SantaGift[]
}

export interface SantaHistoryEvent {
  id: number
  mode: SantaMode
  deadline: string
  ended_at: string
  gift_tiers: number[]
  pairings: SantaPairing[]
}

// --- pacts ----------------------------------------------------------------------------

export type PactKindKey = 'bet' | 'group_bet' | 'resolution'
export type PactStatus =
  | 'proposed'
  | 'active'
  | 'awaiting_result'
  | 'resolved'
  | 'declined'
  | 'cancelled'
  | 'void'
export type PactParticipantState =
  | 'invited'
  | 'requested'
  | 'active'
  | 'settled'
  | 'void'
  | 'declined'
  | 'rejected'
  | 'withdrawn'
  | 'expired'
export type PactProposalState = 'pending' | 'confirmed' | 'disputed' | 'superseded' | 'overruled'

/** What a person puts into a pact. Amounts are in grosze; `side` is only used by sided kinds. */
export interface PactTerms {
  stake_amount?: number | null
  stake_note?: string
  side?: string
}

export interface PactListItem {
  id: number
  kind: PactKindKey
  title: string
  condition: string
  due_at: string | null
  status: PactStatus
  creator: UserBrief
  created_at: string
  resolved_at: string | null
  active_count: number
  my: {
    participant_id: number | null
    role: 'host' | 'opponent' | null
    state: PactParticipantState | null
  }
}

export interface PactParticipant {
  id: number
  user: UserBrief
  role: 'host' | 'opponent'
  side: string
  stake_amount: number | null
  stake_note: string
  state: PactParticipantState
}

export interface PactProposal {
  id: number
  /** The wager it settles (bets); null when it settles the whole pact. */
  wager_id: number | null
  proposed_by: UserBrief
  /** Shape depends on the kind; `{ void: true }` calls it off. */
  result: Record<string, unknown>
  state: PactProposalState
  created_at: string
  decided_at: string | null
  ruling_poll_id: number | null
  confirmed_by: number[]
}

export interface PactAttachment {
  id: number
  url: string
  caption: string
  uploaded_by: UserBrief
  created_at: string
}

/** What the signed-in user can do on a pact right now (decided by the server). */
export interface PactActions {
  can_respond: boolean
  can_request_join: boolean
  /** Resolutions only: once the deadline has passed. */
  can_call_judgment: boolean
  /** False before the deadline: until then a pact can only be called off. */
  can_set_result: boolean
  can_withdraw_request: boolean
  can_attach: boolean
  /** Participant ids whose join request the user must decide. */
  to_decide: number[]
  /** Proposal ids waiting for the user's confirmation. */
  to_confirm: number[]
  /** Disputed proposal ids the user may put to a Sejmik vote. */
  can_escalate: number[]
}

export interface PactDetail extends PactListItem {
  notes: string
  config: { sides?: string[] }
  outcome: Record<string, unknown> | null
  participants: PactParticipant[]
  proposals: PactProposal[]
  actions: PactActions
  attachments: PactAttachment[]
  /** Resolutions: the Sejmik vote in which every other member is judging it, if one is running. */
  judgment_poll_id: number | null
}

export interface PactInvitee extends PactTerms {
  user_id: number
}

export interface PactCreatePayload {
  kind: PactKindKey
  title: string
  condition: string
  notes: string
  due_at: string | null
  config: { sides?: string[] }
  /** The creator's own side and stake (group bets). */
  host?: PactTerms
  opponents: PactInvitee[]
  /** TEMPORARY, superusers only: create the pact as this member (an old pact, entered by hand). */
  creator_id?: number
}

export interface PactRespondPayload extends PactTerms {
  accept: boolean
}

export interface PactKindStats {
  won: number
  lost: number
  draw: number
}

/** A member's record across settled pacts (administrators see it on the profile). */
export interface PactStats extends PactKindStats {
  /** Grosze won / lost on pacts. */
  money_won: number
  money_lost: number
  by_kind: Partial<Record<PactKindKey, PactKindStats>>
}

// --- ledger ---------------------------------------------------------------------------
// Mirrors `backend/ledger`: an entry (what happened) means obligations (who owes whom), and the
// balances are the sum of the obligations of confirmed entries. Money in the base currency (PLN)
// travels as integer grosze; an entry's own `amount` is in minor units of its `currency`.

export type LedgerKindKey = 'expense' | 'income' | 'debt' | 'payment'
/** The kinds with payers and items: an expense, and an income (a negative expense, where the
 * "payers" are who received the money). */
export type SharedKindKey = 'expense' | 'income'
/** Only `confirmed` entries count. A payment is `pending` until the receiver confirms it. */
export type LedgerEntryStatus = 'pending' | 'confirmed' | 'rejected' | 'cancelled'
export type LedgerCategory =
  | 'lodging'
  | 'bills'
  | 'groceries'
  | 'fun'
  | 'health'
  | 'insurance'
  | 'transport'
  | 'food'
  | 'shopping'
  | 'weed'
  | 'other'
/** A category as the server describes it; the emoji is its icon, separate from the name. */
export interface LedgerCategoryInfo {
  key: LedgerCategory
  label: string
  emoji: string
}
/** How an item is divided: evenly, by weights, or into exact amounts. */
export type SplitKind = 'equal' | 'shares' | 'exact'

export interface ExpensePayer {
  user_id: number
  amount: number
}

export interface ExpenseShare {
  user_id: number
  /** Ignored for `equal`; the amount itself for `exact`. */
  weight: number
}

export interface ExpenseItem {
  name: string
  amount: number
  split: SplitKind
  shares: ExpenseShare[]
}

export interface ExpenseDetails {
  payers: ExpensePayer[]
  items: ExpenseItem[]
}

export interface DebtDetails {
  debts: { debtor_id: number; creditor_id: number; amount: number; item: string }[]
}

export interface PaymentDetails {
  from: number
  to: number
}

/** `debtor` owes `creditor` `amount` (grosze, or a quantity of `item`) because of the entry. */
export interface LedgerObligation {
  debtor: UserBrief
  creditor: UserBrief
  amount: number
  item: string
}

/** One person in an expense: what they paid and their part, in the entry's currency and in PLN. */
export interface LedgerShareRow {
  user: UserBrief
  paid: number
  owed: number
  paid_base: number
  owed_base: number
}

export interface LedgerAttachment {
  id: number
  url: string
  uploaded_by: UserBrief
  created_at: string
}

/** What the signed-in user can do with the entry right now (decided by the server). */
export interface LedgerActions {
  confirm: boolean
  reject: boolean
  cancel: boolean
  edit: boolean
  attach: boolean
}

interface LedgerEntryBase {
  id: number
  status: LedgerEntryStatus
  title: string
  note: string
  category: LedgerCategory | ''
  /** `YYYY-MM-DD`. */
  occurred_on: string
  /** Minor units of `currency`, or a quantity of `item`; null when an entry mixes units. */
  amount: number | null
  /** Empty for goods and mixed entries. */
  currency: string
  item: string
  /** `amount` in grosze; null for goods and mixed entries. */
  base_amount: number | null
  /** PLN for one unit of `currency` (decimal string); null for PLN. */
  rate: string | null
  /** The day the rate was published (a weekend uses Friday's). */
  rate_date: string | null
  source_type: string
  source_id: number | null
  created_by: UserBrief | null
  created_at: string
  updated_at: string
  decided_at: string | null
  /** Send it back when editing; a newer one on the server means somebody else saved first. */
  version: number
  obligations: LedgerObligation[]
  attachments: LedgerAttachment[]
  actions: LedgerActions
}

export type ExpenseEntry = LedgerEntryBase & {
  kind: 'expense'
  details: ExpenseDetails
  breakdown: LedgerShareRow[]
}
/** Same shape as an expense; in its breakdown `paid` is what someone received and `owed` the
 * part that belongs to them. */
export type IncomeEntry = LedgerEntryBase & {
  kind: 'income'
  details: ExpenseDetails
  breakdown: LedgerShareRow[]
}
export type SharedEntry = ExpenseEntry | IncomeEntry
export type DebtEntry = LedgerEntryBase & { kind: 'debt'; details: DebtDetails; breakdown: null }
export type PaymentEntry = LedgerEntryBase & {
  kind: 'payment'
  details: PaymentDetails
  breakdown: null
}
export type LedgerEntry = ExpenseEntry | IncomeEntry | DebtEntry | PaymentEntry

/** An expense or an income (`payers`: who received it). */
export interface ExpenseInput {
  kind: SharedKindKey
  title: string
  currency: string
  occurred_on: string
  category: LedgerCategory
  note: string
  payers: ExpensePayer[]
  items: ExpenseItem[]
}

/** A debt you are a party to: money in `currency` (empty `item`) or a quantity of `item`. */
export interface DebtInput {
  kind: 'debt'
  debtor_id: number
  creditor_id: number
  title: string
  item: string
  amount: number
  currency: string
  occurred_on: string
  note: string
}

export interface PaymentInput {
  kind: 'payment'
  to_user_id: number
  amount: number
  item: string
  note: string
}

export type LedgerEntryInput = ExpenseInput | DebtInput | PaymentInput

/** One person's total in one unit. `net` > 0: others owe them; `net` < 0: they owe others. */
export interface LedgerMemberBalance {
  user: UserBrief
  /** Empty for money (grosze). */
  item: string
  net: number
}

/** `debtor` should give `creditor` `amount` (grosze, or a quantity of `item`). */
export interface LedgerTransfer {
  debtor: UserBrief
  creditor: UserBrief
  item: string
  amount: number
}

export interface LedgerPendingPayment extends LedgerTransfer {
  entry_id: number
}

/**
 * The group's Bilans. `members` counts confirmed entries; `settlements` (who should pay whom, the
 * fewest transfers for money, per pair for goods) already assume `pending` payments go through.
 */
export interface LedgerBalances {
  currency: string
  members: LedgerMemberBalance[]
  settlements: LedgerTransfer[]
  pending: LedgerPendingPayment[]
}

export interface LedgerCurrency {
  code: string
  /** Minor-unit digits: 2 for PLN, 0 for JPY. */
  exponent: number
}

export interface LedgerMeta {
  base_currency: string
  currencies: LedgerCurrency[]
  categories: LedgerCategoryInfo[]
}

export interface LedgerRate {
  currency: string
  rate: string
  rate_date: string
}

/** A slice of the stats: an entry category (or `debts`), net of incomes, in grosze. */
export interface LedgerStatsCategory {
  key: LedgerCategory | 'debts'
  label: string
  emoji: string
  spent: number
  count: number
}

export interface LedgerStatsPerson {
  user: UserBrief
  /** Their part of what was spent. */
  share: number
  /** What they paid out. */
  paid: number
}

/** The group's spending over a period (grosze): expenses and money debts minus incomes. */
export interface LedgerStats {
  currency: string
  start: string
  end: string
  /** What `series` is bucketed by. */
  unit: 'day' | 'month'
  spent: number
  expenses: number
  income: number
  count: number
  /** Null when the period is a single month or less. */
  monthly_average: number | null
  categories: LedgerStatsCategory[]
  people: LedgerStatsPerson[]
  /** `period` is `YYYY-MM-DD` (a day) or `YYYY-MM` (a month). */
  series: { period: string; spent: number }[]
}

/** What the receipt scan reads off a photo: every field can be missing; amounts are decimals as printed. */
export interface ScannedReceipt {
  merchant: { name: string | null } | null
  items: ScannedItem[] | null
  total: number | null
  /** ISO 4217 code, when the model could tell. */
  currency: string | null
  /** ISO 639-1 code of the receipt's language. */
  language: string | null
}

export interface ScannedItem {
  name: string | null
  quantity: number | null
  unit_price: number | null
  total: number | null
}

/** SSE `ledger.ocr.request`: the server just sent request number `attempt` for scan `job_id`. */
export interface OcrRequestEvent {
  job_id: string
  attempt: number
}

export interface TricountParticipant {
  name: string
  /** Suggested member (same username), if any. */
  user_id: number | null
}

export interface TricountPreview {
  title: string
  participants: TricountParticipant[]
  expenses: number
  incomes: number
  payments: number
  already_imported: number
  skipped_deleted: number
  attachments: number
  currencies: string[]
  first_date: string | null
  last_date: string | null
}

export interface TricountResult {
  imported: number
  skipped_existing: number
  skipped_deleted: number
}
