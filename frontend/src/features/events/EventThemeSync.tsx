import { useEffect } from 'react'

import { useBirthdays } from '@/features/birthdays/hooks'
import { birthdayFor, type ThemeId } from '@/themes'
import { useTheme } from '@/themes/context'
import { BirthdayLayer } from '@/themes/BirthdayLayer'
import { CapybaraLayer } from '@/themes/CapybaraLayer'
import { EasterLayer } from '@/themes/EasterLayer'
import { HalloweenLayer } from '@/themes/HalloweenLayer'
import { NewYearLayer } from '@/themes/NewYearLayer'
import { PolandLayer } from '@/themes/PolandLayer'
import { SnowLayer } from '@/themes/SnowLayer'

/** Whether an event is running, or `undefined` while that isn't known yet. */
type EventStatus = boolean | undefined

/**
 * Events that dress the whole app in a theme while they run, highest priority first. To add one,
 * read its active flag here. A seasonal theme still beats all of them (see `getSeasonalTheme`).
 */
function useEventThemes(): [EventStatus, ThemeId][] {
  const birthdays = useBirthdays().data
  const { selected } = useTheme()
  return [[birthdays && birthdays.length > 0, birthdayFor(selected)]]
}

/**
 * The theme of the first running event, `null` if none runs, or `undefined` while an event that
 * could still win is loading (keep the remembered override until then, so nothing flashes).
 */
function pickEventTheme(events: [EventStatus, ThemeId][]): ThemeId | null | undefined {
  for (const [active, theme] of events) {
    if (active === undefined) return undefined
    if (active) return theme
  }
  return null
}

/**
 * Forces the running event's theme over whatever the user picked; their pick comes back when the
 * event ends. The override is remembered, so a reload paints it before the status has been fetched.
 * Also renders the effective theme's decorations (snow for christmas, bats and embers for halloween, petals for polska, eggs for easter, steam and oranges for capybara, balloons for birthday, fireworks for newyear).
 */
export function EventThemeSync() {
  const { theme, setOverride } = useTheme()
  const eventTheme = pickEventTheme(useEventThemes())

  useEffect(() => {
    if (eventTheme !== undefined) setOverride(eventTheme)
  }, [eventTheme, setOverride])

  if (theme === 'christmas') return <SnowLayer />
  if (theme === 'newyear') return <NewYearLayer />
  if (theme === 'halloween') return <HalloweenLayer />
  if (theme === 'polska') return <PolandLayer />
  if (theme === 'easter') return <EasterLayer />
  if (theme === 'birthday' || theme === 'birthday-dusk') return <BirthdayLayer />
  if (theme === 'capybara' || theme === 'capybara-dusk') return <CapybaraLayer />
  return null
}
