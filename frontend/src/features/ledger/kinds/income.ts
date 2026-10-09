import { Banknote } from 'lucide-react'

import { sharedKind } from '@/features/ledger/kinds/shared'
import type { IncomeEntry } from '@/lib/api-types'

/** Money somebody received that belongs to several people: a negative expense. */
export const incomeKind = sharedKind<IncomeEntry>('income', Banknote)
