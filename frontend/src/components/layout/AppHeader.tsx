import { UserPanel } from '@/components/layout/UserPanel'

interface AppHeaderProps {
  /** Shown next to the logo, e.g. "Management". */
  section?: string
}

export function AppHeader({ section }: AppHeaderProps) {
  return (
    <header className="sticky top-0 z-10 flex h-14 items-center justify-between border-b bg-card/90 px-4 pt-[env(safe-area-inset-top)] backdrop-blur">
      <div className="flex items-baseline gap-3">
        <h1 className="font-logo text-xl font-bold tracking-wider uppercase">stowarzysz</h1>
        {section && <span className="text-sm text-muted-foreground">{section}</span>}
      </div>
      <UserPanel inManage={section !== undefined} />
    </header>
  )
}
