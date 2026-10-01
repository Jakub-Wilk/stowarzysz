import { ChevronRight, Plus } from 'lucide-react'
import { Link } from 'react-router'

import { Stagger } from '@/components/motion/Stagger'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useManagedUsers } from '@/features/admin/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { BackLink } from '@/components/layout/BackLink'
import { VotingStatsLine } from '@/features/admin/VotingStats'

function Badge({ children }: { children: string }) {
  return (
    <span className="rounded-full border px-2 py-0.5 text-sm text-muted-foreground">
      {children}
    </span>
  )
}

export function UsersPage() {
  const { data: users, isPending, isError, refetch } = useManagedUsers()

  return (
    <>
      <BackLink to="/manage">Manage</BackLink>
      <div className="mb-4 flex items-center justify-between">
        <h2 className="text-xl font-semibold">Users</h2>
        <Button nativeButton={false} render={<Link to="/manage/users/new" />}>
          <Plus /> Add user
        </Button>
      </div>
      {isPending && (
        <div className="flex flex-col gap-2" role="status" aria-label="Loading users">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-20" />
          ))}
        </div>
      )}
      {isError && (
        <div className="flex flex-col items-start gap-2">
          <span role="alert" className="text-sm text-destructive">
            Couldn't load users.
          </span>
          <Button variant="outline" onClick={() => refetch()}>
            Retry
          </Button>
        </div>
      )}
      {users && (
        <Stagger as="ul" className="flex flex-col gap-2">
          {users.map((user) => (
            <Link
              key={user.id}
              to={`/manage/users/${user.id}`}
              className="group flex items-center gap-4 rounded-lg border bg-card p-3 transition-all duration-(--duration-base) ease-(--ease-out-soft) hover:-translate-y-0.5 hover:bg-accent hover:shadow-lg focus-visible:border-ring focus-visible:outline-none"
            >
              <UserAvatar username={user.username} src={user.avatar_url} size="md" />
              <div className="flex min-w-0 flex-col gap-1">
                <span className="text-metal truncate text-lg font-medium">{user.username}</span>
                <span className="flex flex-wrap items-center gap-1.5">
                  {user.is_superuser && <Badge>Superuser</Badge>}
                  {!user.is_active && <Badge>Inactive</Badge>}
                  {!user.has_password && <Badge>Awaiting activation</Badge>}
                </span>
                <VotingStatsLine stats={user.voting} />
              </div>
              <ChevronRight
                className="ml-auto size-5 shrink-0 transition-transform group-hover:translate-x-1"
                aria-hidden
              />
            </Link>
          ))}
        </Stagger>
      )}
    </>
  )
}
