import type { PactKindUI } from '@/features/pacts/kinds/types'

/** A goal a group commits to together; anyone in it can claim "done" at any time. */
export const sharedGoal: PactKindUI = {
  key: 'shared_goal',
  label: 'Wspólny cel',
  blurb:
    'Cel całej grupy, nawet za wiele lat. Każdy uczestnik może zgłosić, że się udało, a pozostali potwierdzają. Bez pieniędzy.',
  money: 'none',
  sided: false,
  wagerBased: false,
  needsDue: true,
  joinable: true,
  judgedByEveryone: false,
  outcomeChoices: () => [
    { label: 'Udało się, cel osiągnięty', result: { done: true }, tone: 'positive', early: true },
    { label: 'Nie udało się', result: { done: false }, tone: 'negative' },
  ],
  describeResult: (result) => {
    if (result.void === true) return 'cel unieważniony'
    return result.done === true ? 'cel osiągnięty' : 'celu nie osiągnięto'
  },
}
