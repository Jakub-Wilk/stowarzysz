import { useEffect, useRef } from 'react'
import { useLocation, useNavigate } from 'react-router'

import { tabs } from '@/components/layout/tabs'

/**
 * React Router numbers its history entries in `history.state.idx`; 0 is the first entry of the
 * tab, the one where the system back button would leave the app.
 */
function historyIndex(): number | undefined {
  return (window.history.state as { idx?: number } | null)?.idx
}

/**
 * When the app starts on a sub page (a notification or a shared link opened `/voting/12`), put
 * its tab under it, so back goes to the list instead of closing the app. Call before the router
 * is created: it writes the entries in the router's own format.
 */
export function insertParentEntry() {
  if (window.history.length > 1) return
  const { pathname, search, hash } = window.location
  const parent = tabs.find((t) => pathname.startsWith(`${t.path}/`))
  if (!parent) return
  window.history.replaceState({ usr: null, key: 'default', idx: 0 }, '', parent.path)
  window.history.pushState({ usr: null, key: 'deep-link', idx: 1 }, '', pathname + search + hash)
}

/**
 * Back on a main screen reloads the page instead of closing the app: whenever a tab is the first
 * history entry, a copy of it is pushed on top, and coming back down to the first entry reloads.
 */
export function useBackToRefresh() {
  const location = useLocation()
  const navigate = useNavigate()
  const armed = useRef(false)

  useEffect(() => {
    if (historyIndex() !== 0) return
    // the copy is pushed again after the reload, by the fresh page
    if (armed.current) {
      window.location.reload()
      return
    }
    armed.current = true
    void navigate(location.pathname)
  }, [location.key, location.pathname, navigate])
}
