import { describeSideResult, sideChoices } from '@/features/pacts/kinds/sided'
import type { PactKindUI } from '@/features/pacts/kinds/types'

/** A shared pot: everyone picks a side and a stake; the winning side splits the losing stakes. */
export const groupBet: PactKindUI = {
  key: 'group_bet',
  label: 'Zakład grupowy',
  blurb:
    'Wspólna pula. Każdy wybiera stronę i stawkę, zwycięzcy dzielą stawki przegranych proporcjonalnie.',
  money: 'pot',
  sided: true,
  wagerBased: false,
  needsDue: false,
  canBeOpen: true,
  outcomeChoices: (pact) => sideChoices(pact),
  describeResult: (result) => describeSideResult(result),
}
