import { ChevronLeft } from 'lucide-react'
import { Link } from 'react-router'

export function BackLink({ to, children }: { to: string; children: string }) {
  return (
    <Link to={to} className="text-metal mb-4 inline-flex items-center gap-1 text-sm">
      <ChevronLeft className="size-4" aria-hidden /> {children}
    </Link>
  )
}
