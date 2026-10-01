import { ArrowLeft, LogOut, Settings } from 'lucide-react'
import { Link } from 'react-router'

import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { useLogout, useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'

/** Avatar button opening the account popover. `inManage` swaps the Management link for a way back. */
export function UserPanel({ inManage = false }: { inManage?: boolean }) {
  const { data: me } = useMe()
  const logout = useLogout()
  if (!me) return null

  return (
    <Popover>
      <PopoverTrigger
        aria-label={`Account: ${me.display_name}`}
        className="rounded-full outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        <UserAvatar name={me.display_name} />
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64 items-center gap-3 p-4 text-center">
        <UserAvatar name={me.display_name} size="lg" className="size-16 text-xl" />
        <div className="flex flex-col">
          <span className="text-base font-semibold">{me.display_name}</span>
          <span className="text-sm text-muted-foreground">@{me.username}</span>
          {me.email && <span className="text-sm text-muted-foreground">{me.email}</span>}
        </div>
        <Button
          variant="outline"
          className="w-full"
          disabled={logout.isPending}
          onClick={() => logout.mutate()}
        >
          <LogOut /> Log out
        </Button>
        {me.is_superuser &&
          (inManage ? (
            <Button
              variant="outline"
              className="w-full"
              nativeButton={false}
              render={<Link to="/" />}
            >
              <ArrowLeft /> Back to app
            </Button>
          ) : (
            <Button
              variant="outline"
              className="w-full"
              nativeButton={false}
              render={<Link to="/manage" />}
            >
              <Settings /> Management
            </Button>
          ))}
      </PopoverContent>
    </Popover>
  )
}
