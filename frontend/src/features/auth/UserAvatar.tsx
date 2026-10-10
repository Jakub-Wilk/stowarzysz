import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import { useIsBirthday } from '@/features/birthdays/hooks'
import { cn } from '@/lib/utils'

// The shadcn Avatar's own `size` variants win over className, so sizes are defined here instead.
const sizes = {
  sm: 'size-6 text-xs',
  default: 'size-11 text-base',
  md: 'size-12 text-base',
  lg: 'size-16 text-xl',
  xl: 'size-24 text-3xl',
} as const

interface UserAvatarProps {
  username: string
  /**
   * Profile picture URL; initials on the theme's metallic fill are shown without one.
   */
  src?: string | null
  size?: keyof typeof sizes
  /** Let the theme dress the picture up (halloween's pumpkin). Only for the signed-in user's own. */
  decorated?: boolean
}

export function UserAvatar({ username, src, size = 'default', decorated }: UserAvatarProps) {
  const birthday = useIsBirthday(username)
  const avatar = (
    <Avatar className={sizes[size]}>
      {src && <AvatarImage src={src} alt="" />}
      <AvatarFallback
        className={cn(
          'bg-primary bg-(image:--metal) font-medium text-primary-foreground',
          sizes[size],
          'size-full',
        )}
      >
        {username.slice(0, 2).toUpperCase() || '?'}
      </AvatarFallback>
      {/* Hidden unless the theme dresses pictures up (halloween's pumpkin, see halloween.css). */}
      {src && decorated && <span aria-hidden data-slot="avatar-overlay" className="hidden" />}
    </Avatar>
  )
  // On their birthday, everyone's picture of them is gift-wrapped (see `.gift-wrap` in index.css).
  if (!birthday) return avatar
  return (
    <span className="gift-wrap" data-size={size}>
      {avatar}
    </span>
  )
}
