import { ChevronRight, Users, type LucideIcon } from 'lucide-react'
import { Link } from 'react-router'

const objectTypes: { path: string; label: string; icon: LucideIcon }[] = [
  { path: '/manage/users', label: 'Users', icon: Users },
]

export function ManageIndexPage() {
  return (
    <>
      <h2 className="mb-4 text-xl font-semibold">Manage</h2>
      <ul className="flex flex-col gap-2">
        {objectTypes.map(({ path, label, icon: Icon }) => (
          <li key={path}>
            <Link
              to={path}
              className="flex items-center gap-4 rounded-lg border bg-card p-4 transition-colors hover:bg-accent focus-visible:border-ring focus-visible:outline-none"
            >
              <Icon className="size-6" aria-hidden />
              <span className="text-metal text-lg font-medium">{label}</span>
              <ChevronRight className="ml-auto size-5" aria-hidden />
            </Link>
          </li>
        ))}
      </ul>
    </>
  )
}
