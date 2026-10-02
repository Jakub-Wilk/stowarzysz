import { Plus } from 'lucide-react'
import { useEffect, useRef } from 'react'
import { Link } from 'react-router'

import { Stagger } from '@/components/motion/Stagger'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { dayGroup } from '@/features/voting/dates'
import { useClosedPolls, useOpenPolls } from '@/features/voting/hooks'
import { PollRow } from '@/features/voting/PollRow'
import type { PollListItem } from '@/lib/api-types'

function groupByDay(polls: PollListItem[]): [string, PollListItem[]][] {
  const groups = new Map<string, PollListItem[]>()
  for (const poll of polls) {
    const label = dayGroup(poll.closed_at ?? poll.created_at)
    groups.set(label, [...(groups.get(label) ?? []), poll])
  }
  return [...groups]
}

function Heading({ children }: { children: string }) {
  return <h3 className="mt-8 mb-3 text-base font-semibold tracking-wide uppercase">{children}</h3>
}

export function VotingPage() {
  const open = useOpenPolls()
  const closed = useClosedPolls()
  const sentinel = useRef<HTMLDivElement>(null)
  const { hasNextPage, isFetchingNextPage, fetchNextPage } = closed

  // Load the next page of history when the end of the list scrolls into view.
  useEffect(() => {
    const node = sentinel.current
    if (!node || !hasNextPage) return
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting) && !isFetchingNextPage) void fetchNextPage()
    })
    observer.observe(node)
    return () => observer.disconnect()
  }, [hasNextPage, isFetchingNextPage, fetchNextPage])

  const history = closed.data?.pages.flatMap((page) => page.results) ?? []
  const nothing = open.data?.length === 0 && closed.isSuccess && history.length === 0

  return (
    <>
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-2xl font-semibold">Sejmik</h2>
        <Button nativeButton={false} render={<Link to="/voting/new" />}>
          <Plus /> Głosowanie
        </Button>
      </div>

      <div>
        {(open.isError || closed.isError) && (
          <div className="mt-6 flex flex-col items-start gap-2">
            <span role="alert" className="text-base text-destructive">
              Nie udało się wczytać porządku obrad.
            </span>
            <Button
              variant="outline"
              onClick={() => {
                void open.refetch()
                void closed.refetch()
              }}
            >
              Spróbuj ponownie
            </Button>
          </div>
        )}

        {open.isPending && closed.isPending && (
          <div
            className="mt-8 flex flex-col gap-3"
            role="status"
            aria-label="Ładowanie porządku obrad"
          >
            {[0, 1, 2].map((i) => (
              <Skeleton key={i} className="h-24" />
            ))}
          </div>
        )}

        {nothing && (
          <p className="mt-10 text-center text-base text-muted-foreground">
            Porządek obrad jest pusty. Zarządź pierwsze głosowanie!
          </p>
        )}

        {open.data && open.data.length > 0 && (
          <section>
            <Heading>Trwające obrady</Heading>
            <Stagger as="ul" className="flex flex-col gap-3">
              {open.data.map((poll) => (
                <PollRow key={poll.id} poll={poll} />
              ))}
            </Stagger>
          </section>
        )}

        {groupByDay(history).map(([label, polls]) => (
          <section key={label}>
            <Heading>{label}</Heading>
            <Stagger as="ul" className="flex flex-col gap-3">
              {polls.map((poll) => (
                <PollRow key={poll.id} poll={poll} />
              ))}
            </Stagger>
          </section>
        ))}

        {hasNextPage && (
          <div ref={sentinel} className="mt-4 flex justify-center">
            <Button variant="ghost" disabled={isFetchingNextPage} onClick={() => fetchNextPage()}>
              {isFetchingNextPage ? 'Ładowanie…' : 'Wczytaj więcej'}
            </Button>
          </div>
        )}
      </div>
    </>
  )
}
