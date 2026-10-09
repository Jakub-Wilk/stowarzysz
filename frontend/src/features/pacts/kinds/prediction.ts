import { describeSideResult, sideChoices } from '@/features/pacts/kinds/sided'
import type { PactKindUI } from '@/features/pacts/kinds/types'

/** A dated claim: everyone picks a side, those who were right score. */
export const prediction: PactKindUI = {
  key: 'prediction',
  label: 'Przewidywanie',
  blurb: 'Bez pieniędzy. Każdy typuje stronę, a po terminie punktują ci, którzy mieli rację.',
  money: 'none',
  sided: true,
  wagerBased: false,
  needsDue: true,
  canBeOpen: true,
  outcomeChoices: (pact) => sideChoices(pact),
  describeResult: (result) => describeSideResult(result),
}
