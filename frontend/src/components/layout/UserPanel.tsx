import { ArrowLeft, Bell, BellOff, Lock, LogOut, Settings } from 'lucide-react'
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
import { cn } from '@/lib/utils'
import { selectableThemes, type ThemeId } from '@/themes'
import { MAX_CHROMA } from '@/themes/color'
import { useTheme } from '@/themes/context'
import { useBaseChroma, useBaseHue } from '@/themes/useBaseHue'

const HUE_GRADIENT = `linear-gradient(to right, ${Array.from(
  { length: 13 },
  (_, i) => `hsl(${i * 30} 100% 50%)`,
).join(', ')})`

/** Grey to vivid in the current base hue (the track follows the hue slider via inheritance). */
const CHROMA_GRADIENT =
  'linear-gradient(to right, hsl(var(--base-hue) 0% 50%), hsl(var(--base-hue) 100% 50%))'

/**
 * Theme picker and base color sliders. All are remembered per browser. While a theme is forced
 * over the user's pick (christmas during Secret Santa) they are shaded and inert.
 */
function AppearanceSettings() {
  const { theme, selected, setTheme, themes } = useTheme()
  const { hue, custom, setHue, reset: resetHue } = useBaseHue()
  const saturation = useBaseChroma()
  const locked = theme !== selected
  const lockedLabel = themes.find((t) => t.id === theme)?.label

  return (
    <div className="relative w-full">
      <div
        inert={locked}
        aria-hidden={locked}
        className={cn(
          'flex w-full flex-col gap-3 text-left transition-opacity',
          locked && 'opacity-30 select-none',
        )}
      >
        <div className="flex flex-col gap-1.5">
          <Label>Motyw</Label>
          <Select
            value={selected}
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
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between">
            <Label>Nasycenie</Label>
            {saturation.custom && (
              <Button variant="ghost" size="xs" onClick={saturation.reset}>
                Domyślne
              </Button>
            )}
          </div>
          <Slider
            aria-label="Nasycenie"
            min={0}
            max={MAX_CHROMA * 100}
            step={1}
            value={Math.round(saturation.chroma * 100)}
            onValueChange={(v) => saturation.setChroma((Array.isArray(v) ? v[0] : v) / 100)}
            style={{ '--hue-gradient': CHROMA_GRADIENT } as React.CSSProperties}
            className="**:data-[slot=slider-range]:hidden **:data-[slot=slider-track]:h-2 **:data-[slot=slider-track]:bg-(image:--hue-gradient)"
          />
        </div>
      </div>
      {locked && (
        <div className="absolute inset-0 flex items-center justify-center p-2">
          <p className="flex items-center gap-1.5 rounded-md border bg-popover px-3 py-2 text-sm font-medium shadow-sm">
            <Lock className="size-4 shrink-0" />
            Motyw zablokowany: {lockedLabel}
          </p>
        </div>
      )}
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
