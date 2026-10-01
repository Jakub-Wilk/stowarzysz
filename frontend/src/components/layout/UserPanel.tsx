import { ArrowLeft, Bell, BellOff, LogOut, Settings } from 'lucide-react'
import { Link } from 'react-router'

import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { useLogout, useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { usePush } from '@/features/push/usePush'

/** Opt this browser in or out of vote notifications. Hidden where it can't work. */
function NotificationsToggle() {
  const { state, enable, disable, busy } = usePush()
  if (state === 'unsupported' || state === 'unconfigured' || state === 'loading') return null

  if (state === 'blocked') {
    return (
      <p className="text-center text-sm text-muted-foreground">
        Notifications are blocked in your browser settings for this site.
      </p>
    )
  }
  const on = state === 'on'
  return (
    <>
      <Button
        variant="outline"
        className="w-full"
        disabled={busy}
        onClick={() => (on ? disable.mutate() : enable.mutate())}
      >
        {on ? <BellOff /> : <Bell />} {on ? 'Turn off notifications' : 'Turn on notifications'}
      </Button>
      {(enable.isError || disable.isError) && (
        <span role="alert" className="text-center text-sm text-destructive">
          Couldn&apos;t change notifications. Try again later.
        </span>
      )}
    </>
  )
}

/** Avatar button opening the account popover. `inManage` swaps the Management link for a way back. */
export function UserPanel({ inManage = false }: { inManage?: boolean }) {
  const { data: me } = useMe()
  const logout = useLogout()
  if (!me) return null

  return (
    <Popover>
      <PopoverTrigger
        aria-label={`Account: ${me.username}`}
        className="rounded-full outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        <UserAvatar username={me.username} src={me.avatar_url} />
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64 items-center gap-3 p-4 text-center">
        <UserAvatar username={me.username} src={me.avatar_url} size="lg" />
        <span className="text-base font-semibold">{me.username}</span>
        <NotificationsToggle />
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
