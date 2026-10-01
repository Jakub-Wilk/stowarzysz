import { Download, LogIn, Share, SquarePlus } from 'lucide-react'
import { Link } from 'react-router'

import { Reveal } from '@/components/motion/Reveal'
import { Button } from '@/components/ui/button'
import { useInstall } from '@/features/install/useInstall'

function InstallSection() {
  const { state, install } = useInstall()

  if (state === 'installed') return null

  if (state === 'prompt') {
    return (
      <Button size="lg" onClick={() => void install()} className="h-12 px-6 text-base">
        <Download /> Zainstaluj aplikację
      </Button>
    )
  }

  if (state === 'ios') {
    return (
      <p className="flex max-w-xs flex-wrap items-center justify-center gap-x-1 text-sm text-muted-foreground">
        Aby zainstalować, kliknij <Share className="size-4" aria-label="Udostępnij" /> w Safari, a
        potem <SquarePlus className="size-4" aria-hidden /> „Do ekranu początkowego”.
      </p>
    )
  }

  return (
    <p className="max-w-xs text-sm text-muted-foreground">
      Aby zainstalować, otwórz menu przeglądarki i wybierz „Zainstaluj aplikację”.
    </p>
  )
}

/** Public landing page: what the app is, how to install it, and the way to log in. */
export function LandingPage() {
  return (
    <div className="flex min-h-svh flex-col">
      <div className="h-1 bg-(image:--metal)" />
      <main className="mx-auto flex w-full max-w-md flex-1 flex-col items-center justify-center gap-8 p-6 py-16 text-center">
        <Reveal className="flex flex-col items-center gap-3">
          <h1 className="font-logo text-4xl font-bold tracking-wider uppercase md:text-5xl">
            stowarzysz
          </h1>
        </Reveal>
        <Reveal index={1} className="flex flex-col items-center gap-4">
          <InstallSection />
          <Button variant="ghost" nativeButton={false} render={<Link to="/login" />}>
            <LogIn /> Zaloguj się
          </Button>
        </Reveal>
      </main>
      <div className="h-1 bg-(image:--metal)" />
    </div>
  )
}
