import { LogOut } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { useLogout, useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'

function UserPanel() {
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
      </PopoverContent>
    </Popover>
  )
}

export function AppHeader() {
  return (
    <header className="sticky top-0 z-10 flex h-14 items-center justify-between border-b bg-card/90 px-4 pt-[env(safe-area-inset-top)] backdrop-blur">
      <h1 className="font-logo text-xl font-bold tracking-wider uppercase">stowarzysz</h1>
      <UserPanel />
    </header>
  )
}
