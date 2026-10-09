import { bet } from '@/features/pacts/kinds/bet'
import { groupBet } from '@/features/pacts/kinds/group_bet'
import { prediction } from '@/features/pacts/kinds/prediction'
import { resolution } from '@/features/pacts/kinds/resolution'
import type { PactKindUI } from '@/features/pacts/kinds/types'
import type { PactKindKey } from '@/lib/api-types'

/** Order is the order of the kind picker. */
export const PACT_KINDS: PactKindUI[] = [bet, groupBet, prediction, resolution]

const byKey = new Map<string, PactKindUI>(PACT_KINDS.map((k) => [k.key, k]))

export function getPactKind(key: PactKindKey | string): PactKindUI {
  return byKey.get(key) ?? bet
}
