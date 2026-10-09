import { DRAW_LABEL, type OutcomeChoice, type PactKindUI } from '@/features/pacts/kinds/types'
import type { PactDetail } from '@/lib/api-types'

function sideChoices(pact: PactDetail): OutcomeChoice[] {
  const sides = pact.config.sides ?? []
  return [
    ...sides.map((side): OutcomeChoice => ({
      label: `Wygrywa „${side}”`,
      result: { winner: side },
      tone: 'positive',
    })),
    { label: DRAW_LABEL, result: { winner: 'draw' }, tone: 'neutral' },
  ]
}

/**
 * Everyone picks a side. With stakes it is a shared pot (the winning side splits the losing
 * stakes); without any it is a prediction that only scores who was right.
 */
export const groupBet: PactKindUI = {
  key: 'group_bet',
  label: 'Zakład grupowy',
  blurb:
    'Każdy wybiera stronę. Ze stawkami zwycięzcy dzielą stawki przegranych proporcjonalnie; bez stawek to samo przewidywanie.',
  money: 'pot',
  sided: true,
  wagerBased: false,
  needsDue: false,
  joinable: true,
  judgedByEveryone: false,
  outcomeChoices: (pact) => sideChoices(pact),
  describeResult: (result) => {
    if (result.void === true) return 'zakład unieważniony'
    return result.winner === 'draw' ? 'remis' : `wygrywa „${String(result.winner)}”`
  },
}
