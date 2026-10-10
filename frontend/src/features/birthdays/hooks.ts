import { useQuery } from '@tanstack/react-query'

import { useHasSession } from '@/features/auth/hooks'
import { apiFetch } from '@/lib/api'
import type { UserBrief } from '@/lib/api-types'

export const birthdaysKey = ['birthdays'] as const

/** Members whose birthday is today (the server decides the day, in Warsaw time). */
export function useBirthdays() {
  const enabled = useHasSession()
  return useQuery({
    queryKey: birthdaysKey,
    queryFn: () => apiFetch<UserBrief[]>('/api/auth/birthdays/'),
    enabled,
  })
}

/** Whether it is this person's birthday today. */
export function useIsBirthday(username: string): boolean {
  const { data } = useBirthdays()
  return data?.some((p) => p.username === username) ?? false
}
