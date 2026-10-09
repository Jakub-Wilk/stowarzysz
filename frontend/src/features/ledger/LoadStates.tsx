import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'

export function ListSkeleton({ label }: { label: string }) {
  return (
    <div className="mt-6 flex flex-col gap-3" role="status" aria-label={label}>
      {[0, 1, 2].map((i) => (
        <Skeleton key={i} className="h-20" />
      ))}
    </div>
  )
}

/** "Nie udało się wczytać {what}." with a retry button. */
export function LoadError({ what, onRetry }: { what: string; onRetry: () => void }) {
  return (
    <div className="mt-6 flex flex-col items-start gap-2">
      <span role="alert" className="text-base text-destructive">
        Nie udało się wczytać {what}.
      </span>
      <Button variant="outline" onClick={onRetry}>
        Spróbuj ponownie
      </Button>
    </div>
  )
}
