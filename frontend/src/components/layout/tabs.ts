import { Gift, Handshake, Vote, Wallet, type LucideIcon } from 'lucide-react'
import type { ComponentType } from 'react'

import { LedgerPage } from '@/pages/LedgerPage'
import { PactsPage } from '@/pages/PactsPage'
import { SecretSantaPage } from '@/pages/SecretSantaPage'
import { VotingPage } from '@/pages/VotingPage'

export interface TabDef {
  path: string
  label: string
  icon: LucideIcon
  page: ComponentType
}

/** Order matters: it is the left-to-right order of the tab bar and of the swipe pager. */
export const tabs: TabDef[] = [
  { path: '/voting', label: 'Sejmik', icon: Vote, page: VotingPage },
  { path: '/pacts', label: 'Umowy', icon: Handshake, page: PactsPage },
  { path: '/ledger', label: 'Kasa Skarbowa', icon: Wallet, page: LedgerPage },
  { path: '/secret-santa', label: 'Secret Santa', icon: Gift, page: SecretSantaPage },
]
