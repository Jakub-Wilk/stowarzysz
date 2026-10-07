import { ArrowLeft, Bell, BellOff, LogOut, Settings } from 'lucide-react'
import { Link } from 'react-router'

import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { useLogout, useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { usePush } from '@/features/push/usePush'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Slider } from '@/components/ui/slider'
import { selectableThemes, type ThemeId } from '@/themes'
import { useTheme } from '@/themes/context'
import { useBaseHue } from '@/themes/useBaseHue'

const HUE_GRADIENT = `linear-gradient(to right, ${Array.from(
  { length: 13 },
  (_, i) => `oklch(0.7 0.15 ${i * 30})`,
).join(', ')})`

/** Theme picker and base hue slider. Both are remembered per browser. */
function AppearanceSettings() {
  const { theme, setTheme } = useTheme()
  const { hue, custom, setHue, reset: resetHue } = useBaseHue()

  return (
    <div className="flex w-full flex-col gap-3 text-left">
      <div className="flex flex-col gap-1.5">
        <Label>Motyw</Label>
        <Select
          value={theme === 'christmas' ? 'royal' : theme}
          onValueChange={(id) => {
            setTheme(id as ThemeId)
          }}
          items={selectableThemes.map((t) => ({ value: t.id, label: t.label }))}
        >
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent alignItemWithTrigger={false}>
            {selectableThemes.map((t) => (
              <SelectItem key={t.id} value={t.id}>
                {t.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center justify-between">
          <Label>Kolor</Label>
          {custom && (
            <Button variant="ghost" size="xs" onClick={resetHue}>
              Domyślny
            </Button>
          )}
        </div>
        <Slider
          aria-label="Kolor"
          min={0}
          max={360}
          step={1}
          value={hue}
          onValueChange={(v) => setHue(Array.isArray(v) ? v[0] : v)}
          style={{ '--hue-gradient': HUE_GRADIENT } as React.CSSProperties}
          className="**:data-[slot=slider-range]:hidden **:data-[slot=slider-track]:h-2 **:data-[slot=slider-track]:bg-(image:--hue-gradient)"
        />
      </div>
    </div>
  )
}

/** Opt this browser in or out of vote notifications. Hidden where it can't work. */
function NotificationsToggle() {
  const { state, enable, disable, busy } = usePush()
  if (state === 'unsupported' || state === 'unconfigured' || state === 'loading') return null

  if (state === 'blocked') {
    return (
      <p className="text-center text-sm text-muted-foreground">
        Powiadomienia są zablokowane w ustawieniach przeglądarki dla tej strony.
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
        {on ? <BellOff /> : <Bell />} {on ? 'Wyłącz powiadomienia' : 'Włącz powiadomienia'}
      </Button>
      {(enable.isError || disable.isError) && (
        <span role="alert" className="text-center text-sm text-destructive">
          Nie udało się zmienić ustawień powiadomień. Spróbuj ponownie później.
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
        aria-label={`Konto: ${me.username}`}
        className="rounded-full outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
      >
        <UserAvatar username={me.username} src={me.avatar_url} />
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64 items-center gap-3 p-4 text-center">
        <UserAvatar username={me.username} src={me.avatar_url} size="lg" />
        <span className="text-base font-semibold">{me.username}</span>
        <AppearanceSettings />
        <NotificationsToggle />
        <Button
          variant="outline"
          className="w-full"
          disabled={logout.isPending}
          onClick={() => logout.mutate()}
        >
          <LogOut /> Wyloguj
        </Button>
        {me.is_superuser &&
          (inManage ? (
            <Button
              variant="outline"
              className="w-full"
              nativeButton={false}
              render={<Link to="/" />}
            >
              <ArrowLeft /> Wróć do aplikacji
            </Button>
          ) : (
            <Button
              variant="outline"
              className="w-full"
              nativeButton={false}
              render={<Link to="/manage" />}
            >
              <Settings /> Zarządzanie
            </Button>
          ))}
      </PopoverContent>
    </Popover>
  )
}
