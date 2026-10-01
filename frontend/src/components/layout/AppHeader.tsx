import { Avatar, AvatarFallback } from '@/components/ui/avatar'
import { useMe } from '@/features/auth/hooks'

export function AppHeader() {
  const { data: me } = useMe()
  const initials = (me?.username ?? '?').slice(0, 2).toUpperCase()

  return (
    <header className="sticky top-0 z-10 flex h-14 items-center justify-between border-b bg-card/90 px-4 pt-[env(safe-area-inset-top)] backdrop-blur">
      <h1 className="font-logo text-xl font-bold tracking-wider uppercase">stowarzysz</h1>
      <Avatar aria-label={me ? `Profile: ${me.username}` : 'Profile'}>
        <AvatarFallback className="bg-primary bg-(image:--metal) font-medium text-primary-foreground">
          {initials}
        </AvatarFallback>
      </Avatar>
    </header>
  )
}
