import { ChevronLeft } from 'lucide-react'
import { Link } from 'react-router'

export function BackLink({ to, children }: { to: string; children: string }) {
  return (
    <Link to={to} className="text-metal mb-4 inline-flex min-h-11 items-center gap-1 text-base">
      <ChevronLeft className="size-5" aria-hidden /> {children}
    </Link>
  )
}
