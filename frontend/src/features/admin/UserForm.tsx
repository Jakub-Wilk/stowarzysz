import { useState, type ReactNode, type SubmitEvent } from 'react'
import { useNavigate } from 'react-router'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { ActivationLinkDialog } from '@/features/admin/ActivationLinkDialog'
import { DeleteUserDialog } from '@/features/admin/DeleteUserDialog'
import {
  useCreateUser,
  useDeleteUser,
  useIssueActivationLink,
  useUpdateUser,
} from '@/features/admin/hooks'
import { useMe } from '@/features/auth/hooks'
import { formErrors } from '@/lib/api-errors'
import type { ActivationLink, ManagedUser, ManagedUserPayload } from '@/lib/api-types'

const EMPTY: ManagedUserPayload = {
  username: '',
  first_name: '',
  last_name: '',
  email: '',
  is_active: true,
  is_superuser: false,
}

function payloadOf(user: ManagedUser): ManagedUserPayload {
  const { username, first_name, last_name, email, is_active, is_superuser } = user
  return { username, first_name, last_name, email, is_active, is_superuser }
}

function nameOf(user: Pick<ManagedUser, 'username' | 'first_name' | 'last_name'>): string {
  return `${user.first_name} ${user.last_name}`.trim() || user.username
}

function TextField({
  id,
  label,
  value,
  onChange,
  error,
  type = 'text',
  required,
}: {
  id: string
  label: string
  value: string
  onChange: (value: string) => void
  error?: string
  type?: string
  required?: boolean
}) {
  return (
    <div className="flex flex-col gap-2">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        type={type}
        value={value}
        required={required}
        aria-invalid={error !== undefined}
        onChange={(e) => onChange(e.target.value)}
      />
      {error && (
        <span role="alert" className="text-sm text-destructive">
          {error}
        </span>
      )}
    </div>
  )
}

function SwitchField({
  label,
  hint,
  checked,
  onChange,
  disabled,
}: {
  label: string
  hint: ReactNode
  checked: boolean
  onChange: (checked: boolean) => void
  disabled?: boolean
}) {
  const labelId = `switch-${label}`
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex flex-col gap-1">
        <span id={labelId} className="text-sm font-medium">
          {label}
        </span>
        <span className="text-sm text-muted-foreground">{hint}</span>
      </div>
      <Switch
        aria-labelledby={labelId}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
      />
    </div>
  )
}

interface LinkDialogState {
  link: ActivationLink
  /** Where to go once the dialog is closed (after creating a user). */
  nextPath?: string
}

/** Create (no `user`) or edit an account, plus link issuing and deletion when editing. */
export function UserForm({ user }: { user?: ManagedUser }) {
  const navigate = useNavigate()
  const { data: me } = useMe()
  const create = useCreateUser()
  const update = useUpdateUser()
  const remove = useDeleteUser()
  const issueLink = useIssueActivationLink()

  const initial = user ? payloadOf(user) : EMPTY
  const [form, setForm] = useState<ManagedUserPayload>(initial)
  const [linkDialog, setLinkDialog] = useState<LinkDialogState | null>(null)
  const [deleteOpen, setDeleteOpen] = useState(false)

  const isSelf = user !== undefined && user.id === me?.id
  const dirty = JSON.stringify(form) !== JSON.stringify(initial)
  const saving = create.isPending || update.isPending
  const errors = formErrors(user ? update.error : create.error)
  const set = <K extends keyof ManagedUserPayload>(key: K, value: ManagedUserPayload[K]) =>
    setForm((f) => ({ ...f, [key]: value }))

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (saving) return
    if (user) {
      update.mutate({ id: user.id, payload: form })
      return
    }
    create.mutate(form, {
      onSuccess: (created) => {
        const editPath = `/manage/users/${created.id}`
        // New accounts have no password: hand the admin the activation link right away.
        issueLink.mutate(created.id, {
          onSuccess: (link) => setLinkDialog({ link, nextPath: editPath }),
          onError: () => navigate(editPath, { replace: true }),
        })
      },
    })
  }

  const closeLinkDialog = () => {
    const next = linkDialog?.nextPath
    setLinkDialog(null)
    if (next) navigate(next, { replace: true })
  }

  return (
    <>
      <form onSubmit={submit} className="flex max-w-md flex-col gap-5">
        <TextField
          id="username"
          label="Username"
          value={form.username}
          onChange={(v) => set('username', v)}
          error={errors.fields.username}
          required
        />
        <div className="grid grid-cols-2 gap-4">
          <TextField
            id="first_name"
            label="First name"
            value={form.first_name}
            onChange={(v) => set('first_name', v)}
            error={errors.fields.first_name}
          />
          <TextField
            id="last_name"
            label="Last name"
            value={form.last_name}
            onChange={(v) => set('last_name', v)}
            error={errors.fields.last_name}
          />
        </div>
        <TextField
          id="email"
          label="Email"
          type="email"
          value={form.email}
          onChange={(v) => set('email', v)}
          error={errors.fields.email}
        />
        <SwitchField
          label="Active"
          hint={isSelf ? "You can't deactivate yourself." : 'Inactive accounts cannot sign in.'}
          checked={form.is_active}
          onChange={(v) => set('is_active', v)}
          disabled={isSelf}
        />
        <SwitchField
          label="Superuser"
          hint={
            isSelf
              ? "You can't remove your own access."
              : 'Can open the management area and edit all accounts.'
          }
          checked={form.is_superuser}
          onChange={(v) => set('is_superuser', v)}
          disabled={isSelf}
        />
        {errors.general && (create.isError || update.isError) && (
          <span role="alert" className="text-sm text-destructive">
            {errors.general}
          </span>
        )}
        <div className="flex items-center gap-3">
          <Button type="submit" disabled={saving || (user !== undefined && !dirty)}>
            {user ? 'Save changes' : 'Create user'}
          </Button>
          {user && update.isSuccess && !dirty && <span className="text-sm">Saved</span>}
        </div>
      </form>

      {user && (
        <section className="mt-10 flex max-w-md flex-col gap-4 border-t pt-6">
          <div className="flex flex-col gap-2">
            <h3 className="font-semibold">{user.has_password ? 'Password reset' : 'Activation'}</h3>
            <p className="text-sm text-muted-foreground">
              {user.has_password
                ? 'Issue a one-time link that lets this user choose a new password.'
                : 'This account has no password yet. Issue a one-time link so the user can set one.'}
            </p>
            <div>
              <Button
                variant="outline"
                disabled={issueLink.isPending}
                onClick={() =>
                  issueLink.mutate(user.id, { onSuccess: (link) => setLinkDialog({ link }) })
                }
              >
                {user.has_password ? 'Issue reset link' : 'Issue activation link'}
              </Button>
            </div>
            {issueLink.isError && (
              <span role="alert" className="text-sm text-destructive">
                {formErrors(issueLink.error).general ?? 'Could not issue a link.'}
              </span>
            )}
          </div>
          <div className="flex flex-col gap-2">
            <h3 className="font-semibold">Delete account</h3>
            <div>
              <Button variant="destructive" disabled={isSelf} onClick={() => setDeleteOpen(true)}>
                Delete user
              </Button>
            </div>
            {isSelf && (
              <span className="text-sm text-muted-foreground">
                You can't delete your own account.
              </span>
            )}
          </div>
        </section>
      )}

      <ActivationLinkDialog
        link={linkDialog?.link ?? null}
        userName={nameOf(user ?? form)}
        isReset={user?.has_password ?? false}
        onClose={closeLinkDialog}
      />
      {user && (
        <DeleteUserDialog
          open={deleteOpen}
          onOpenChange={(open) => {
            setDeleteOpen(open)
            if (!open) remove.reset()
          }}
          userName={nameOf(user)}
          pending={remove.isPending}
          error={remove.isError ? (formErrors(remove.error).general ?? 'Delete failed.') : null}
          onConfirm={() =>
            remove.mutate(user.id, {
              onSuccess: () => navigate('/manage/users', { replace: true }),
            })
          }
        />
      )}
    </>
  )
}
