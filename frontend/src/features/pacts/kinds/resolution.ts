import type { PactKindUI } from '@/features/pacts/kinds/types'

/** A personal resolution: the host commits, every other member judges by vote. Pride only. */
export const resolution: PactKindUI = {
  key: 'resolution',
  label: 'Postanowienie',
  blurb:
    'Zobowiązujesz się do czegoś, a po terminie wszyscy posłowie głosują, czy dotrzymałeś słowa. Bez pieniędzy.',
  money: 'none',
  sided: false,
  wagerBased: false,
  needsDue: true,
  joinable: false,
  judgedByEveryone: true,
  outcomeChoices: () => [
    { label: 'Dotrzymano postanowienia', result: { kept: true }, tone: 'positive' },
    { label: 'Nie dotrzymano postanowienia', result: { kept: false }, tone: 'negative' },
  ],
  describeResult: (result, pact) => {
    if (result.void === true) return 'postanowienie unieważnione'
    return result.kept === true
      ? `${pact.creator.username} dotrzymał(a) słowa`
      : `${pact.creator.username} nie dotrzymał(a) słowa`
  },
}
