import { ChevronDown } from 'lucide-react'
import { useState, type SubmitEvent } from 'react'

import { Stagger } from '@/components/motion/Stagger'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { useMe } from '@/features/auth/hooks'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { formatAmount, formatAmounts, formatDate } from '@/features/secretsanta/format'
import { useSantaHistory, useSetGiftNote } from '@/features/secretsanta/hooks'
import type { SantaGift, SantaHistoryEvent } from '@/lib/api-types'
import { cn } from '@/lib/utils'

function GiftNote({ gift, canEdit }: { gift: SantaGift; canEdit: boolean }) {
  const save = useSetGiftNote()
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(gift.note)

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    save.mutate({ id: gift.id, note: draft.trim() }, { onSuccess: () => setEditing(false) })
  }

  if (editing) {
    return (
      <form onSubmit={submit} className="flex items-center gap-2">
        <Input
          value={draft}
          maxLength={300}
          autoFocus
          aria-label={`Co podarowano za ${formatAmount(gift.amount)}`}
          onChange={(e) => setDraft(e.target.value)}
        />
        <Button type="submit" disabled={save.isPending}>
          Zapisz
        </Button>
        <Button
          type="button"
          variant="ghost"
          onClick={() => {
            setDraft(gift.note)
            setEditing(false)
          }}
        >
          Anuluj
        </Button>
      </form>
    )
  }

  return (
    <div className="flex items-center justify-between gap-2">
      <span className={cn('text-base', !gift.note && 'text-muted-foreground')}>
        {gift.note || 'Brak opisu'}
      </span>
      {canEdit && (
        <Button type="button" variant="ghost" size="sm" onClick={() => setEditing(true)}>
          {gift.note ? 'Edytuj' : 'Dodaj opis'}
        </Button>
      )}
    </div>
  )
}

function HistoryCard({ event }: { event: SantaHistoryEvent }) {
  const { data: me } = useMe()
  const [open, setOpen] = useState(false)

  return (
    <div className="rounded-lg border bg-card">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-4 p-4 text-left focus-visible:outline-none"
      >
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <span className="text-metal text-lg font-semibold">
            Secret Santa, {formatDate(event.ended_at)}
          </span>
          <span className="text-sm text-muted-foreground">
            Kwoty: {formatAmounts(event.gift_tiers)}
          </span>
        </div>
        <ChevronDown
          className={cn('size-5 shrink-0 transition-transform', open && 'rotate-180')}
          aria-hidden
        />
      </button>
      {open && (
        <Stagger as="ul" className="flex flex-col gap-4 border-t p-4">
          {event.pairings.map((pairing) => {
            const canEdit = me !== undefined && (me.is_superuser || me.id === pairing.giver.id)
            return (
              <div key={pairing.giver.id} className="flex flex-col gap-2">
                <div className="flex flex-wrap items-center gap-2">
                  <UserAvatar
                    username={pairing.giver.username}
                    src={pairing.giver.avatar_url}
                    size="sm"
                  />
                  <span className="text-metal text-base font-medium">{pairing.giver.username}</span>
                  <span aria-label="obdarował">→</span>
                  <UserAvatar
                    username={pairing.receiver.username}
                    src={pairing.receiver.avatar_url}
                    size="sm"
                  />
                  <span className="text-metal text-base font-medium">
                    {pairing.receiver.username}
                  </span>
                </div>
                <ul className="flex flex-col gap-1 pl-2">
                  {pairing.gifts.map((gift) => (
                    <li key={gift.id} className="flex flex-col gap-1">
                      <span className="text-sm font-semibold text-muted-foreground">
                        {formatAmount(gift.amount)}
                      </span>
                      <GiftNote gift={gift} canEdit={canEdit} />
                    </li>
                  ))}
                </ul>
              </div>
            )
          })}
        </Stagger>
      )}
    </div>
  )
}

/** Earlier rounds, newest first. Pairings are public once a round has ended. */
export function SantaHistory() {
  const history = useSantaHistory()
  const events = history.data?.pages.flatMap((page) => page.results) ?? []

  if (history.isPending) return <Skeleton className="h-20" />
  if (events.length === 0) return null

  return (
    <section className="flex flex-col gap-3">
      <h3 className="text-lg font-semibold">Poprzednie edycje</h3>
      <Stagger className="flex flex-col gap-3">
        {events.map((event) => (
          <HistoryCard key={event.id} event={event} />
        ))}
      </Stagger>
      {history.hasNextPage && (
        <div>
          <Button
            variant="outline"
            disabled={history.isFetchingNextPage}
            onClick={() => history.fetchNextPage()}
          >
            Pokaż starsze
          </Button>
        </div>
      )}
    </section>
  )
}
