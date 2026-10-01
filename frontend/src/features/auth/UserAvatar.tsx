import { Avatar, AvatarFallback } from '@/components/ui/avatar'
import { cn } from '@/lib/utils'

function initialsOf(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  const letters = parts.length > 1 ? parts[0][0] + parts[parts.length - 1][0] : name.slice(0, 2)
  return letters.toUpperCase() || '?'
}

interface UserAvatarProps {
  name: string
  size?: 'default' | 'sm' | 'lg'
  className?: string
}

/** Initials on the theme's metallic fill; swap in an image here once users have pictures. */
export function UserAvatar({ name, size, className }: UserAvatarProps) {
  return (
    <Avatar size={size} className={className}>
      <AvatarFallback
        className={cn(
          'bg-primary bg-(image:--metal) font-medium text-primary-foreground',
          size === 'lg' && 'text-base',
        )}
      >
        {initialsOf(name)}
      </AvatarFallback>
    </Avatar>
  )
}
