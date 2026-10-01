import { ArrowRight, ChevronLeft, ChevronRight } from 'lucide-react'
import { useEffect, useRef, useState, type SubmitEvent } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { ApiError } from '@/lib/api'
import type { LoginUser } from '@/lib/api-types'
import { useLogin, useLoginUsers } from '@/features/auth/hooks'
import { Brand } from '@/features/auth/Brand'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { cn } from '@/lib/utils'

function loginErrorMessage(error: unknown): string {
  if (error instanceof ApiError && error.status === 401) return 'Incorrect password.'
  return "Couldn't sign in. Please try again."
}

const SLIDE_MS = 300

function PasswordForm({
  user,
  active,
  onBack,
}: {
  user: LoginUser
  active: boolean
  onBack: () => void
}) {
  const [password, setPassword] = useState('')
  const login = useLogin()
  const { reset } = login
  const input = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (active) {
      input.current?.focus({ preventScroll: true }) // the clipped slider must not scroll itself
      return
    }
    // Once slid out of view, forget the typed password and any error.
    const id = setTimeout(() => {
      setPassword('')
      reset()
    }, SLIDE_MS)
    return () => clearTimeout(id)
  }, [active, reset])

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!password || login.isPending) return
    login.mutate({ username: user.username, password })
  }

  return (
    <form onSubmit={submit} className="flex flex-col items-center gap-5">
      <UserAvatar username={user.username} src={user.avatar_url} size="xl" />
      <h2 className="text-2xl font-semibold">{user.username}</h2>
      <div className="flex w-full max-w-xs gap-2">
        <Input
          type="password"
          autoComplete="current-password"
          placeholder="Password"
          aria-label="Password"
          aria-invalid={login.isError}
          ref={input}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className="h-10 flex-1"
        />
        <Button
          type="submit"
          size="icon-lg"
          aria-label="Sign in"
          disabled={!password || login.isPending}
          className="h-10 w-10"
        >
          <ArrowRight />
        </Button>
      </div>
      <span role="alert" className="min-h-5 text-sm text-destructive">
        {login.isError ? loginErrorMessage(login.error) : ''}
      </span>
      <Button type="button" variant="ghost" onClick={onBack}>
        <ChevronLeft /> Switch user
      </Button>
    </form>
  )
}

function UserList({ onSelect }: { onSelect: (user: LoginUser) => void }) {
  const { data: users, isPending, isError, refetch } = useLoginUsers()

  if (isPending) return <span className="text-sm text-muted-foreground">Loading users…</span>
  if (isError) {
    return (
      <div className="flex flex-col items-start gap-2">
        <span className="text-sm text-destructive">Couldn't load users.</span>
        <Button variant="outline" onClick={() => refetch()}>
          Retry
        </Button>
      </div>
    )
  }
  if (users.length === 0) {
    return <span className="text-sm text-muted-foreground">No accounts have been set up yet.</span>
  }

  return (
    <ul className="flex w-[calc(100%+1.5rem)] max-w-sm flex-col gap-2">
      {users.map((user) => (
        <li key={user.username}>
          <button
            type="button"
            onClick={() => onSelect(user)}
            className={cn(
              'flex w-full items-center gap-4 rounded-lg border border-transparent p-3 text-left transition-colors',
              'hover:border-border hover:bg-card focus-visible:border-ring focus-visible:bg-card focus-visible:outline-none',
            )}
          >
            <UserAvatar username={user.username} src={user.avatar_url} size="md" />
            <span className="text-metal text-lg font-medium">{user.username}</span>
            <ChevronRight className="ml-auto size-5" aria-hidden />
          </button>
        </li>
      ))}
    </ul>
  )
}

/** Pick-your-account login: list the accounts, then ask for that account's password. */
export function LoginPage() {
  // The user stays set while sliding back, so the password pane doesn't empty mid-animation.
  const [user, setUser] = useState<LoginUser | null>(null)
  const [open, setOpen] = useState(false)
  const tagline = open ? 'Enter your password to continue' : 'Select your account to begin'

  const pane = 'flex w-full shrink-0 flex-col items-center justify-center px-3 md:items-start'

  return (
    <div className="flex min-h-svh flex-col">
      <div className="h-1 bg-(image:--metal)" />
      {/* Mobile: brand on top, content centered on the screen (an invisible copy of the brand
          below balances the top one). Desktop: two columns. */}
      <div className="mx-auto grid w-full max-w-4xl flex-1 grid-rows-[auto_1fr_auto] justify-center gap-10 p-6 py-16 md:grid-cols-2 md:grid-rows-none md:items-center md:gap-16 md:py-6">
        <Brand tagline={tagline} />
        <div className="flex flex-col md:min-h-64 md:justify-center md:border-l md:pl-16">
          {/* w-0 + min-w keeps the two panes from widening the grid column (it follows the brand). */}
          <div className="-mx-3 w-0 min-w-[calc(100%+1.5rem)] flex-1 overflow-hidden md:flex-none">
            <div
              className="flex h-full transition-transform ease-out motion-reduce:transition-none"
              style={{
                transform: `translateX(${open ? '-100%' : '0'})`,
                transitionDuration: `${SLIDE_MS}ms`,
              }}
            >
              <div className={pane} inert={open}>
                <UserList
                  onSelect={(u) => {
                    setUser(u)
                    setOpen(true)
                  }}
                />
              </div>
              <div className={pane} inert={!open}>
                {user && (
                  <PasswordForm
                    key={user.username}
                    user={user}
                    active={open}
                    onBack={() => setOpen(false)}
                  />
                )}
              </div>
            </div>
          </div>
        </div>
        <div aria-hidden className="invisible md:hidden">
          <Brand tagline={tagline} />
        </div>
      </div>
      <div className="h-1 bg-(image:--metal)" />
    </div>
  )
}
