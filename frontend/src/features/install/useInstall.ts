import { useSyncExternalStore } from 'react'

/** Chromium's install prompt event; not in lib.dom yet. */
interface BeforeInstallPromptEvent extends Event {
  prompt: () => Promise<void>
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>
}

export type InstallState = 'installed' | 'prompt' | 'ios' | 'unavailable'

let deferred: BeforeInstallPromptEvent | null = null
let installed = false
const listeners = new Set<() => void>()
const emit = () => listeners.forEach((l) => l())

const standaloneQuery =
  typeof window !== 'undefined' ? window.matchMedia('(display-mode: standalone)') : null

// Registered at module load: the event can fire before any component mounts.
if (typeof window !== 'undefined') {
  window.addEventListener('beforeinstallprompt', (e) => {
    e.preventDefault()
    deferred = e as BeforeInstallPromptEvent
    emit()
  })
  window.addEventListener('appinstalled', () => {
    deferred = null
    installed = true
    emit()
  })
  standaloneQuery?.addEventListener('change', emit)
}

const isStandalone = () =>
  installed ||
  standaloneQuery?.matches === true ||
  (navigator as Navigator & { standalone?: boolean }).standalone === true

// iPadOS reports itself as a Mac with touch support.
const isIos = () =>
  /iphone|ipad|ipod/i.test(navigator.userAgent) ||
  (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1)

function snapshot(): InstallState {
  if (isStandalone()) return 'installed'
  if (deferred) return 'prompt'
  if (isIos()) return 'ios'
  return 'unavailable'
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** PWA install state; `install()` opens the native prompt where the browser supports it. */
export function useInstall() {
  const state = useSyncExternalStore(subscribe, snapshot)

  const install = async () => {
    if (!deferred) return
    const event = deferred
    deferred = null // a prompt can only be used once
    await event.prompt()
    await event.userChoice
    emit()
  }

  return { state, install }
}
