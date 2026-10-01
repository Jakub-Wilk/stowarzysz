import { cn } from '@/lib/utils'

function Skeleton({ className, ...props }: React.ComponentProps<'div'>) {
  return (
    <div
      data-slot="skeleton"
      aria-hidden
      className={cn('animate-shimmer rounded-lg bg-muted', className)}
      {...props}
    />
  )
}

export { Skeleton }
