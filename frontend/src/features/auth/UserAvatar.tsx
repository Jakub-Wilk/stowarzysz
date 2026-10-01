import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar'
import { cn } from '@/lib/utils'

// The shadcn Avatar's own `size` variants win over className, so sizes are defined here instead.
const sizes = {
  sm: 'size-6 text-xs',
  default: 'size-8 text-sm',
  md: 'size-12 text-base',
  lg: 'size-16 text-xl',
  xl: 'size-24 text-3xl',
} as const

interface UserAvatarProps {
  username: string
  /** Profile picture URL; initials on the theme's metallic fill are shown without one. */
  src?: string | null
  size?: keyof typeof sizes
}

export function UserAvatar({ username, src, size = 'default' }: UserAvatarProps) {
  return (
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
    </Avatar>
  )
}
