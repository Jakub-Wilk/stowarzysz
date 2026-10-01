import { Gift, Vote, type LucideIcon } from 'lucide-react'
import type { ComponentType } from 'react'

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
  { path: '/voting', label: 'Voting', icon: Vote, page: VotingPage },
  { path: '/secret-santa', label: 'Secret Santa', icon: Gift, page: SecretSantaPage },
]
