import { useEffect } from 'react'

import { SnowLayer } from '@/features/secretsanta/SnowLayer'
import { useSanta } from '@/features/secretsanta/hooks'
import { useTheme } from '@/themes/context'

/**
 * The whole app wears the Christmas theme while a Secret Santa event is active, whatever theme the
 * user picked; their pick comes back when the event ends. The override is remembered, so a reload
 * paints it before the status has been fetched.
 */
export function SantaThemeSync() {
  const { data } = useSanta()
  const { theme, setOverride } = useTheme()
  const active = data?.active

  useEffect(() => {
    if (active !== undefined) setOverride(active ? 'christmas' : null)
  }, [active, setOverride])

  return theme === 'christmas' ? <SnowLayer /> : null
}
