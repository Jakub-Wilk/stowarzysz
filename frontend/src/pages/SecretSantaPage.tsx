import { Gift, TreePine } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'react-router'

import { Reveal } from '@/components/motion/Reveal'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { SantaHistory } from '@/features/secretsanta/SantaHistory'
import { daysUntil, formatAmount, formatDateTime } from '@/features/secretsanta/format'
import { useSanta } from '@/features/secretsanta/hooks'
import { GiverHelp, HelpRequestsForMe } from '@/features/secretsanta/SantaHelp'
import type { SantaGiverHelp, SantaState, SantaTierVictim, UserBrief } from '@/lib/api-types'
import { DAY_FORMS, plural } from '@/lib/plural'

function Waiting({ isSuperuser }: { isSuperuser: boolean }) {
  return (
    <Reveal className="flex flex-col items-center gap-3 rounded-lg border bg-card px-6 py-12 text-center">
      <TreePine className="size-12" aria-hidden />
      <p className="text-lg font-semibold">Czekamy na święta…</p>
      <p className="max-w-xs text-base text-muted-foreground">
        Tu pojawi się losowanie Secret Santa, gdy tylko nadejdzie czas.
      </p>
      {isSuperuser && (
        <Button nativeButton={false} render={<Link to="/manage/secret-santa" />}>
          Rozpocznij Secret Santa
        </Button>
      )}
    </Reveal>
  )
}

function Deadline({ iso }: { iso: string }) {
  const days = daysUntil(iso)
  const left =
    days > 0 ? `Zostało ${days} ${plural(days, DAY_FORMS)}` : days === 0 ? 'Termin mija dziś' : null
  return (
    <p className="text-base">
      Termin: <span className="font-semibold">{formatDateTime(iso)}</span>
      {left && <span className="text-muted-foreground"> · {left}</span>}
    </p>
  )
}

const helpFor = (help: SantaGiverHelp[], victim: UserBrief) =>
  help.find((h) => h.victim_id === victim.id)

/** The pairing is hidden until tapped, so it isn't read over a shoulder. */
function Victim({
  victim,
  tiers,
  help,
}: {
  victim: UserBrief
  tiers: number[]
  help: SantaGiverHelp[]
}) {
  const [shown, setShown] = useState(false)
  return (
    <div className="flex flex-col items-center gap-4 rounded-lg border bg-card px-6 py-8 text-center">
      <Gift className="size-10" aria-hidden />
      <p className="text-base text-muted-foreground">Twój podopieczny</p>
      {shown ? (
        <div key="shown" className="animate-pop-in flex flex-col items-center gap-3">
          <UserAvatar username={victim.username} src={victim.avatar_url} size="xl" />
          <span className="text-metal text-2xl font-bold">{victim.username}</span>
          <ul className="flex flex-col gap-1 text-lg">
            {tiers.map((amount) => (
              <li key={amount}>Prezent za {formatAmount(amount)}</li>
            ))}
          </ul>
          <GiverHelp victimId={victim.id} help={helpFor(help, victim)} />
          <Button variant="ghost" onClick={() => setShown(false)}>
            Ukryj
          </Button>
        </div>
      ) : (
        <Button onClick={() => setShown(true)}>Pokaż mojego podopiecznego</Button>
      )}
    </div>
  )
}

/** `per_tier` mode: a different victim for each amount. */
function TierVictims({ victims, help }: { victims: SantaTierVictim[]; help: SantaGiverHelp[] }) {
  const [shown, setShown] = useState(false)
  return (
    <div className="flex flex-col items-center gap-4 rounded-lg border bg-card px-6 py-8 text-center">
      <Gift className="size-10" aria-hidden />
      <p className="text-base text-muted-foreground">Twoi podopieczni</p>
      {shown ? (
        <div key="shown" className="animate-pop-in flex flex-col items-center gap-4">
          <ul className="flex flex-col gap-4">
            {victims.map(({ amount, victim }) => (
              <li key={amount} className="flex flex-col items-center gap-2">
                <UserAvatar username={victim.username} src={victim.avatar_url} size="xl" />
                <span className="text-metal text-2xl font-bold">{victim.username}</span>
                <span className="text-lg">Prezent za {formatAmount(amount)}</span>
                <GiverHelp victimId={victim.id} help={helpFor(help, victim)} />
              </li>
            ))}
          </ul>
          <Button variant="ghost" onClick={() => setShown(false)}>
            Ukryj
          </Button>
        </div>
      ) : (
        <Button onClick={() => setShown(true)}>Pokaż moich podopiecznych</Button>
      )}
    </div>
  )
}

function Active({ state, isSuperuser }: { state: SantaState; isSuperuser: boolean }) {
  const { event, my_victim: victim, my_tier_victims: tierVictims, my_help_requests: help } = state
  if (!event) return null
  return (
    <>
      <Reveal className="flex flex-col gap-2">
        <h2 className="text-xl font-semibold">Secret Santa trwa!</h2>
        <Deadline iso={event.deadline} />
        <p className="text-base">
          Kwoty prezentów:{' '}
          <span className="font-semibold">{event.gift_tiers.map(formatAmount).join(', ')}</span>
        </p>
      </Reveal>
      {state.help_requests_for_me.length > 0 && (
        <Reveal index={1}>
          <HelpRequestsForMe requests={state.help_requests_for_me} />
        </Reveal>
      )}
      <Reveal index={1}>
        {victim ? (
          <Victim victim={victim} tiers={event.gift_tiers} help={help} />
        ) : tierVictims.length > 0 ? (
          <TierVictims victims={tierVictims} help={help} />
        ) : (
          <p className="rounded-lg border bg-card p-4 text-base text-muted-foreground">
            Nie bierzesz udziału w tym losowaniu.
          </p>
        )}
      </Reveal>
      {isSuperuser && (
        <Reveal index={2}>
          <Button
            variant="outline"
            nativeButton={false}
            render={<Link to="/manage/secret-santa" />}
          >
            Zarządzaj Secret Santa
          </Button>
        </Reveal>
      )}
    </>
  )
}

export function SecretSantaPage() {
  const { data, isPending, error } = useSanta()
  const { data: me } = useMe()
  const isSuperuser = me?.is_superuser ?? false

  return (
    <div className="flex flex-col gap-6">
      {isPending && <Skeleton className="h-40" />}
      {error && (
        <span role="alert" className="text-sm text-destructive">
          Nie udało się wczytać Secret Santa.
        </span>
      )}
      {data &&
        (data.event ? (
          <Active state={data} isSuperuser={isSuperuser} />
        ) : (
          <Waiting isSuperuser={isSuperuser} />
        ))}
      <SantaHistory />
    </div>
  )
}
