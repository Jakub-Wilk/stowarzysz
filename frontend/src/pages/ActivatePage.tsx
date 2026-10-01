import { useState, type SubmitEvent } from 'react'
import { Link, useNavigate, useParams } from 'react-router'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Brand } from '@/features/auth/Brand'
import { useActivate, useValidateActivation } from '@/features/auth/hooks'
import { formErrors } from '@/lib/api-errors'

function SetPasswordForm({ token, username }: { token: string; username: string }) {
  const navigate = useNavigate()
  const activate = useActivate()
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const mismatch = confirm !== '' && password !== confirm

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!password || mismatch || activate.isPending) return
    // Signs the user in with the returned tokens, then enter the app.
    activate.mutate({ token, password }, { onSuccess: () => navigate('/', { replace: true }) })
  }

  const errors = formErrors(activate.error)
  const message = mismatch
    ? "Passwords don't match."
    : activate.isError
      ? (errors.fields.password ?? errors.fields.token ?? errors.general)
      : null

  return (
    <form onSubmit={submit} className="flex w-full max-w-xs flex-col items-center gap-4">
      <h2 className="text-2xl font-semibold">Welcome, {username}</h2>
      <Input
        type="password"
        autoComplete="new-password"
        placeholder="New password"
        aria-label="New password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        className="h-10"
      />
      <Input
        type="password"
        autoComplete="new-password"
        placeholder="Repeat password"
        aria-label="Repeat password"
        aria-invalid={mismatch}
        value={confirm}
        onChange={(e) => setConfirm(e.target.value)}
        className="h-10"
      />
      <span role="alert" className="min-h-5 text-center text-sm text-destructive">
        {message}
      </span>
      <Button
        type="submit"
        className="h-10 w-full"
        disabled={!password || mismatch || activate.isPending}
      >
        Set password and sign in
      </Button>
    </form>
  )
}

/** Landing page for one-time activation / password-reset links. Works while signed out. */
export function ActivatePage() {
  const { token = '' } = useParams()
  const validation = useValidateActivation(token)

  return (
    <div className="flex min-h-svh flex-col">
      <div className="h-1 bg-(image:--metal)" />
      <div className="mx-auto flex w-full max-w-md flex-1 flex-col items-center justify-center gap-10 p-6">
        <Brand tagline="Set your password" />
        {validation.isPending && <span className="text-sm">Checking your link…</span>}
        {validation.isError && (
          <div className="flex flex-col items-center gap-4 text-center">
            <span role="alert" className="text-sm text-destructive">
              This link is invalid or has expired. Ask an administrator for a new one.
            </span>
            <Button variant="outline" nativeButton={false} render={<Link to="/login" />}>
              Go to sign in
            </Button>
          </div>
        )}
        {validation.data && <SetPasswordForm token={token} username={validation.data.username} />}
      </div>
      <div className="h-1 bg-(image:--metal)" />
    </div>
  )
}
