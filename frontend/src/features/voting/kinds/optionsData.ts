import type { Ballot, PollResult } from '@/lib/api-types'

/** Mirrors the backend limits (`voting/kinds.py`); the server stays the authority. */
export const MIN_OPTIONS = 2
export const MAX_OPTIONS = 10
export const MAX_OPTIONS_OPEN = 20
export const MAX_LABEL_LENGTH = 60

export interface PollOption {
  id: string
  label: string
}

export interface OptionRow extends PollOption {
  /** Votes (single, multi) or first places (ranked). */
  count: number
  /** Ranked only: how many other options this one beats. */
  wins?: number
}

export interface OptionsResult {
  votes_cast: number
  abstain_count: number
  options: OptionRow[]
  winners: string[]
  /** Ranked only: ballots preferring the first option over the second. */
  pairwise: Record<string, Record<string, number>> | null
  condorcet_winner: string | null
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

const isStringArray = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((item) => typeof item === 'string')

export function readOptions(config: Record<string, unknown>): PollOption[] {
  if (!Array.isArray(config.options)) return []
  return config.options.flatMap((option: unknown) =>
    isRecord(option) && typeof option.id === 'string' && typeof option.label === 'string'
      ? [{ id: option.id, label: option.label }]
      : [],
  )
}

export function readResult(result: PollResult | null): OptionsResult | null {
  if (!result || typeof result.votes_cast !== 'number' || !Array.isArray(result.options)) {
    return null
  }
  const options = result.options.flatMap((row: unknown): OptionRow[] =>
    isRecord(row) &&
    typeof row.id === 'string' &&
    typeof row.label === 'string' &&
    typeof row.count === 'number'
      ? [
          {
            id: row.id,
            label: row.label,
            count: row.count,
            wins: typeof row.wins === 'number' ? row.wins : undefined,
          },
        ]
      : [],
  )
  return {
    votes_cast: result.votes_cast,
    abstain_count: typeof result.abstain_count === 'number' ? result.abstain_count : 0,
    options,
    winners: isStringArray(result.winners) ? result.winners : [],
    pairwise: isRecord(result.pairwise)
      ? (result.pairwise as Record<string, Record<string, number>>)
      : null,
    condorcet_winner: typeof result.condorcet_winner === 'string' ? result.condorcet_winner : null,
  }
}

/** The option ids a ballot picked (single, multi) or ranked (ranked), in ballot order. */
export function readPicks(ballot: Ballot | null | undefined): string[] {
  if (!ballot) return []
  if (typeof ballot.choice === 'string') return [ballot.choice]
  if (isStringArray(ballot.choices)) return ballot.choices
  if (isStringArray(ballot.ranking)) return ballot.ranking
  return []
}

export const isOpenPool = (config: Record<string, unknown>): boolean => config.open_options === true

export const readMaxChoices = (config: Record<string, unknown>): number | null =>
  typeof config.max_choices === 'number' ? config.max_choices : null

export const readTieBreakOf = (config: Record<string, unknown>): number | null =>
  typeof config.tie_break_of === 'number' ? config.tie_break_of : null
