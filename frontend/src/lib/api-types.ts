// Hand-written for now; could be generated from /api/schema/ (openapi-typescript).

export interface TokenPair {
  access: string
  refresh: string
}

export interface Me {
  id: number
  username: string
  is_superuser: boolean
  /** Site-relative URL of the profile picture, if one is set. */
  avatar_url: string | null
}

/** Public, minimal account info for the login screen. */
export interface LoginUser {
  username: string
  avatar_url: string | null
}

export interface LoginPayload {
  username: string
  password: string
}

export interface ActivationValidateResult {
  username: string
}

export interface ActivationCompletePayload {
  token: string
  password: string
}

/** Account as seen by a superuser in the management UI. */
export interface ManagedUser {
  id: number
  username: string
  is_active: boolean
  is_superuser: boolean
  /** False until the user has set a password via an activation link. */
  has_password: boolean
  avatar_url: string | null
}

export type ManagedUserPayload = Omit<ManagedUser, 'id' | 'has_password' | 'avatar_url'>

export interface ActivationLink {
  token: string
  url: string
  expires_at: string
}
