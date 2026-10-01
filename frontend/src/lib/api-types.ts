// Hand-written for now; could be generated from /api/schema/ (openapi-typescript).

export interface TokenPair {
  access: string
  refresh: string
}

export interface Me {
  id: number
  username: string
  email: string
  display_name: string
  is_superuser: boolean
}

/** Public, minimal account info for the login screen. */
export interface LoginUser {
  username: string
  display_name: string
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
  first_name: string
  last_name: string
  email: string
  is_active: boolean
  is_superuser: boolean
  /** False until the user has set a password via an activation link. */
  has_password: boolean
}

export type ManagedUserPayload = Omit<ManagedUser, 'id' | 'has_password'>

export interface ActivationLink {
  token: string
  url: string
  expires_at: string
}
