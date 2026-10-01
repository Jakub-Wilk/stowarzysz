import { Plus } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router'

import { Stagger } from '@/components/motion/Stagger'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { dayGroup } from '@/features/voting/dates'
import { useClosedPolls, useOpenPolls } from '@/features/voting/hooks'
import { PollRow } from '@/features/voting/PollRow'
import type { PollListItem } from '@/lib/api-types'
import { cn } from '@/lib/utils'

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

function Scope({ mine, onChange }: { mine: boolean; onChange: (mine: boolean) => void }) {
  return (
    <div role="radiogroup" aria-label="Show" className="relative flex rounded-lg border p-0.5">
      <span
        aria-hidden
        className={cn(
          'absolute inset-y-0.5 left-0.5 w-[calc(50%-0.125rem)] rounded-md bg-primary transition-transform duration-(--duration-base) ease-(--ease-spring)',
          mine && 'translate-x-full',
        )}
      />
      {[
        { label: 'All', value: false },
        { label: 'Mine', value: true },
      ].map(({ label, value }) => (
        <button
          key={label}
          type="button"
          role="radio"
          aria-checked={mine === value}
          onClick={() => onChange(value)}
          className={cn(
            'relative min-h-11 flex-1 rounded-md px-4 text-base transition-colors',
            mine === value ? 'text-primary-foreground' : 'hover:bg-accent/60',
          )}
        >
          {label}
        </button>
      ))}
    </div>
  )
}

export function VotingPage() {
  const [mine, setMine] = useState(false)
  const open = useOpenPolls(mine)
  const closed = useClosedPolls(mine)
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
        <h2 className="text-2xl font-semibold">Voting</h2>
        <div className="flex items-center gap-2">
          <Scope mine={mine} onChange={setMine} />
          <Button nativeButton={false} render={<Link to="/voting/new" />}>
            <Plus /> New vote
          </Button>
        </div>
      </div>

      {(open.isError || closed.isError) && (
        <div className="mt-6 flex flex-col items-start gap-2">
          <span role="alert" className="text-base text-destructive">
            Couldn&apos;t load votes.
          </span>
          <Button
            variant="outline"
            onClick={() => {
              void open.refetch()
              void closed.refetch()
            }}
          >
            Retry
          </Button>
        </div>
      )}

      {open.isPending && closed.isPending && (
        <div className="mt-8 flex flex-col gap-3" role="status" aria-label="Loading votes">
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="h-24" />
          ))}
        </div>
      )}

      {nothing && (
        <p className="mt-10 text-center text-base text-muted-foreground">
          No votes yet. Call the first one!
        </p>
      )}

      {open.data && open.data.length > 0 && (
        <section>
          <Heading>Active</Heading>
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
            {isFetchingNextPage ? 'Loading…' : 'Load more'}
          </Button>
        </div>
      )}
    </>
  )
}
