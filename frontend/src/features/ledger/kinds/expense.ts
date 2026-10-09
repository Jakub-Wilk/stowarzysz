import { Receipt } from 'lucide-react'

import { sharedKind } from '@/features/ledger/kinds/shared'
import type { ExpenseEntry } from '@/lib/api-types'

export const expenseKind = sharedKind<ExpenseEntry>('expense', Receipt)
