import { ImagePlus, Trash2 } from 'lucide-react'
import { useRef, useState, type ChangeEvent } from 'react'

import { Button } from '@/components/ui/button'
import { readPreview, validateImage, type PendingPicture } from '@/features/admin/avatar'
import { useRemoveAvatar, useUploadAvatar } from '@/features/admin/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { formErrors } from '@/lib/api-errors'
import type { ManagedUser } from '@/lib/api-types'

interface AvatarFieldProps {
  /** Shown as initials when there is no picture. */
  username: string
  /** Existing account: changes are saved immediately. Omit when creating. */
  user?: ManagedUser
  /** Creating: the picture waits here until the account exists. */
  pending: PendingPicture | null
  onPendingChange: (picture: PendingPicture | null) => void
}

export function AvatarField({ username, user, pending, onPendingChange }: AvatarFieldProps) {
  const input = useRef<HTMLInputElement>(null)
  const upload = useUploadAvatar()
  const remove = useRemoveAvatar()
  const [clientError, setClientError] = useState<string | null>(null)

  const current = pending?.preview ?? user?.avatar_url ?? null
  const busy = upload.isPending || remove.isPending
  const serverError = upload.isError
    ? formErrors(upload.error)
    : remove.isError
      ? formErrors(remove.error)
      : null
  const error = clientError ?? serverError?.fields.avatar ?? serverError?.general ?? null

  const onFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const chosen = e.target.files?.[0]
    if (!chosen) return
    const problem = validateImage(chosen)
    setClientError(problem)
    // Copy into memory first: the File from an <input> may stop being readable once the input is
    // reset (or later, when a creating form uploads after the account exists).
    const file = problem
      ? chosen
      : new File([await chosen.arrayBuffer()], chosen.name, { type: chosen.type })
    e.target.value = '' // allow choosing the same file again
    if (problem) return
    upload.reset()
    remove.reset()
    if (user) {
      upload.mutate({ id: user.id, file })
    } else {
      onPendingChange({ file, preview: await readPreview(file) })
    }
  }

  const removePicture = () => {
    setClientError(null)
    upload.reset()
    if (user) remove.mutate(user.id)
    else onPendingChange(null)
  }

  return (
    <div className="flex items-center gap-5">
      <UserAvatar username={username} src={current} size="xl" />
      <div className="flex flex-col items-start gap-2">
        <input
          ref={input}
          type="file"
          accept="image/*"
          className="hidden"
          aria-label="Profile picture file"
          onChange={onFile}
        />
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={busy}
            onClick={() => input.current?.click()}
          >
            <ImagePlus /> {current ? 'Change picture' : 'Add picture'}
          </Button>
          {current && (
            <Button type="button" variant="ghost" disabled={busy} onClick={removePicture}>
              <Trash2 /> Remove
            </Button>
          )}
        </div>
        {busy && <span className="text-sm text-muted-foreground">Saving…</span>}
        {error && (
          <span role="alert" className="text-sm text-destructive">
            {error}
          </span>
        )}
      </div>
    </div>
  )
}
