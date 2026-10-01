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

export interface CreateUserPayload {
  username: string
  email?: string
  first_name?: string
  last_name?: string
}

export interface CreatedUser {
  id: number
  username: string
  email: string
  first_name: string
  last_name: string
}

export interface ActivationLink {
  token: string
  url: string
  expires_at: string
}
