import { Check, Copy } from 'lucide-react'
import { useState } from 'react'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import type { ActivationLink } from '@/lib/api-types'

interface ActivationLinkDialogProps {
  /** The link to show; the dialog is open while this is set. */
  link: ActivationLink | null
  userName: string
  /** True when the account already has a password (the link then resets it). */
  isReset: boolean
  /** Optional warning shown under the link. */
  note?: string
  onClose: () => void
}

export function ActivationLinkDialog({
  link,
  userName,
  isReset,
  note,
  onClose,
}: ActivationLinkDialogProps) {
  const [copied, setCopied] = useState<'yes' | 'failed' | null>(null)

  const copy = async () => {
    if (!link) return
    try {
      await navigator.clipboard.writeText(link.url)
      setCopied('yes')
    } catch {
      setCopied('failed') // e.g. insecure context; the user can still select the text
    }
  }

  return (
    <Dialog
      open={link !== null}
      onOpenChange={(open) => {
        if (open) return
        setCopied(null)
        onClose()
      }}
    >
      <DialogContent showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{isReset ? 'Password reset link' : 'Activation link'}</DialogTitle>
          <DialogDescription>
            Send this one-time link to {userName}. They use it to set their password
            {link && ` before ${new Date(link.expires_at).toLocaleString()}`}. It won't be shown
            again, and issuing a new link voids this one.
          </DialogDescription>
        </DialogHeader>
        <div className="flex gap-2">
          <Input
            readOnly
            value={link?.url ?? ''}
            aria-label="Link"
            onFocus={(e) => e.currentTarget.select()}
          />
          <Button type="button" variant="outline" onClick={copy}>
            {copied === 'yes' ? <Check /> : <Copy />} {copied === 'yes' ? 'Copied' : 'Copy'}
          </Button>
        </div>
        {copied === 'failed' && (
          <span role="alert" className="text-sm text-destructive">
            Couldn't copy automatically. Select the link and copy it manually.
          </span>
        )}
        {note && <span className="text-sm text-muted-foreground">{note}</span>}
        <DialogFooter showCloseButton />
      </DialogContent>
    </Dialog>
  )
}
