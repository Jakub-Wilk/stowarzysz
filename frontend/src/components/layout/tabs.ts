import { Gift, Handshake, Vote, type LucideIcon } from 'lucide-react'
import type { ComponentType } from 'react'

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
  { path: '/pacts', label: 'Zakłady', icon: Handshake, page: PactsPage },
  { path: '/secret-santa', label: 'Secret Santa', icon: Gift, page: SecretSantaPage },
]
