import { useEffect, useState } from 'react'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'

/** Seconds the Delete button stays locked after the dialog opens. */
const LOCK_SECONDS = 5

interface DeleteUserDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Shown in the prompt. */
  userName: string
  onConfirm: () => void
  pending: boolean
  error: string | null
}

/**
 * Lives inside the dialog content, which only mounts while the dialog is open, so the countdown
 * restarts on every open without any reset logic.
 */
function LockedActions({
  onConfirm,
  pending,
}: Pick<DeleteUserDialogProps, 'onConfirm' | 'pending'>) {
  const [remaining, setRemaining] = useState(LOCK_SECONDS)

  useEffect(() => {
    const id = setInterval(() => setRemaining((r) => Math.max(r - 1, 0)), 1000)
    return () => clearInterval(id)
  }, [])

  return (
    <AlertDialogFooter>
      <AlertDialogCancel>Anuluj</AlertDialogCancel>
      <AlertDialogAction
        variant="destructive"
        disabled={remaining > 0 || pending}
        onClick={onConfirm}
      >
        {remaining > 0 ? `Usuń (${remaining})` : 'Usuń'}
      </AlertDialogAction>
    </AlertDialogFooter>
  )
}

/** Destructive confirmation whose confirm button unlocks only after a short countdown. */
export function DeleteUserDialog({
  open,
  onOpenChange,
  userName,
  onConfirm,
  pending,
  error,
}: DeleteUserDialogProps) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Usunąć użytkownika {userName}?</AlertDialogTitle>
          <AlertDialogDescription>
            Konto zostanie trwale usunięte, a użytkownik zostanie wylogowany ze wszystkich urządzeń.
            Tej operacji nie można cofnąć.
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error && (
          <span role="alert" className="text-sm text-destructive">
            {error}
          </span>
        )}
        <LockedActions onConfirm={onConfirm} pending={pending} />
      </AlertDialogContent>
    </AlertDialog>
  )
}
