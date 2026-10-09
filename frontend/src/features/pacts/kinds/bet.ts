import { DRAW_LABEL, type PactKindUI } from '@/features/pacts/kinds/types'

/** The host bets the condition happens, each opponent bets it doesn't, one wager each. */
export const bet: PactKindUI = {
  key: 'bet',
  label: 'Zakład',
  blurb: 'Ty kontra każdy z przeciwników, każdy ze swoją stawką. Przegrany płaci stawkę wygranemu.',
  money: 'wager',
  sided: false,
  wagerBased: true,
  needsDue: false,
  canBeOpen: true,
  outcomeChoices: (pact, wager) => {
    const opponent = wager?.user.username ?? 'przeciwnik'
    return [
      {
        label: `Wygrywa ${pact.creator.username} (warunek spełniony)`,
        result: { winner: 'host' },
        tone: 'positive',
      },
      { label: `Wygrywa ${opponent}`, result: { winner: 'opponent' }, tone: 'negative' },
      { label: DRAW_LABEL, result: { winner: 'draw' }, tone: 'neutral' },
    ]
  },
  describeResult: (result, pact, wager) => {
    if (result.void === true) return 'zakład unieważniony'
    if (result.winner === 'host') return `wygrywa ${pact.creator.username}`
    if (result.winner === 'opponent') return `wygrywa ${wager?.user.username ?? 'przeciwnik'}`
    return 'remis'
  },
}
