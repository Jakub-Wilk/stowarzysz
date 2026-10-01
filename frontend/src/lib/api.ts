import type { TokenPair } from '@/lib/api-types'
import {
  clearSession,
  getAccessToken,
  getRefreshToken,
  hasSession,
  setTokens,
} from '@/lib/auth-tokens'

export class ApiError extends Error {
  readonly status: number
  readonly body: unknown

  constructor(status: number, body: unknown) {
    super(`API error ${status}`)
    this.name = 'ApiError'
    this.status = status
    this.body = body
  }
}

async function parseBody(res: Response): Promise<unknown> {
  if (res.status === 204) return undefined
  const text = await res.text()
  if (!text) return undefined
  try {
    return JSON.parse(text)
  } catch {
    return text
  }
}

async function requestRefresh(): Promise<string | null> {
  // Read the refresh token only now: refresh tokens rotate and are blacklisted after use, so a
  // value read before waiting on the cross-tab lock may already have been spent by another tab.
  const refresh = getRefreshToken()
  if (!refresh) return null

  const res = await fetch('/api/auth/token/refresh/', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ refresh }),
  })
  if (res.status === 400 || res.status === 401) {
    clearSession() // refresh token expired, blacklisted or revoked
    return null
  }
  if (!res.ok) throw new ApiError(res.status, await parseBody(res)) // transient: keep the session
  const pair = (await res.json()) as TokenPair
  setTokens(pair)
  return pair.access
}

let refreshInFlight: Promise<string | null> | null = null

/** Single-flight within a tab, serialized across tabs with the Web Locks API. */
export function refreshAccessToken(): Promise<string | null> {
  refreshInFlight ??= (
    'locks' in navigator
      ? navigator.locks.request('token-refresh', requestRefresh)
      : requestRefresh()
  ).finally(() => {
    refreshInFlight = null
  })
  return refreshInFlight
}

/**
 * `fetch` with a Bearer token: refreshes first if only a refresh token is held (e.g. after a
 * reload), and on a 401 refreshes and retries once. Shared by `apiFetch` and the SSE client.
 */
export async function authorizedFetch(
  input: RequestInfo | URL,
  init?: RequestInit,
): Promise<Response> {
  const send = (token: string | null) => {
    const headers = new Headers(init?.headers)
    if (token) headers.set('Authorization', `Bearer ${token}`)
    return fetch(input, { ...init, headers })
  }

  const token = getAccessToken() ?? (hasSession() ? await refreshAccessToken() : null)
  const res = await send(token)
  if (res.status !== 401 || !hasSession()) return res

  const fresh = await refreshAccessToken()
  return fresh ? send(fresh) : res
}

export interface ApiOptions extends Omit<RequestInit, 'body'> {
  /** JSON request body. */
  json?: unknown
  /** Send the Bearer token and refresh on 401. Disable for public endpoints. Default: true. */
  auth?: boolean
}

export async function apiFetch<T>(
  path: string,
  { json, auth = true, ...init }: ApiOptions = {},
): Promise<T> {
  const headers = new Headers(init.headers)
  let body: string | undefined
  if (json !== undefined) {
    headers.set('Content-Type', 'application/json')
    body = JSON.stringify(json)
  }

  const res = await (auth ? authorizedFetch : fetch)(path, { ...init, headers, body })
  const data = await parseBody(res)
  if (!res.ok) throw new ApiError(res.status, data)
  return data as T
}
