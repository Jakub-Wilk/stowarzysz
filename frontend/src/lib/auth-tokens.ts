import type { TokenPair } from '@/lib/api-types'

// Access token: memory only. Refresh token: localStorage (survives reloads and is shared
// across tabs). Trade-off: script injection could read the refresh token.
const REFRESH_KEY = 'auth.refresh'

let accessToken: string | null = null
const listeners = new Set<() => void>()

function emit() {
  listeners.forEach((listener) => listener())
}

function readRefresh(): string | null {
  try {
    return localStorage.getItem(REFRESH_KEY)
  } catch {
    return null
  }
}

function writeRefresh(token: string | null) {
  try {
    if (token === null) localStorage.removeItem(REFRESH_KEY)
    else localStorage.setItem(REFRESH_KEY, token)
  } catch {
    // storage unavailable; session lasts until reload
  }
}

export const getAccessToken = () => accessToken
export const getRefreshToken = readRefresh

/** A session exists while we hold a refresh token, even if the access token has expired. */
export const hasSession = () => readRefresh() !== null

export function setTokens(pair: TokenPair) {
  accessToken = pair.access
  writeRefresh(pair.refresh)
  emit()
}

export function setAccessToken(token: string) {
  accessToken = token
}

export function clearSession() {
  accessToken = null
  writeRefresh(null)
  emit()
}

/** Subscribe to session changes (login, logout, forced logout, other-tab changes). */
export function subscribeSession(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

// Another tab logged in/out or rotated the refresh token.
if (typeof window !== 'undefined') {
  window.addEventListener('storage', (event) => {
    if (event.key !== REFRESH_KEY && event.key !== null) return
    if (readRefresh() === null) accessToken = null
    emit()
  })
}
