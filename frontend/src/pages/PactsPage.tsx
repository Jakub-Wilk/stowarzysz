import { Plus, Wallet } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'react-router'

import { Stagger } from '@/components/motion/Stagger'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { isArchived } from '@/features/pacts/format'
import { usePacts } from '@/features/pacts/hooks'
import { PactRow } from '@/features/pacts/PactRow'
import { Ranking } from '@/features/pacts/Ranking'
import type { PactListItem } from '@/lib/api-types'
import { PACT_FORMS, plural } from '@/lib/plural'

type Segment = 'running' | 'archive' | 'ranking'

const SEGMENTS: { key: Segment; label: string }[] = [
  { key: 'running', label: 'Trwające' },
  { key: 'archive', label: 'Archiwum' },
  { key: 'ranking', label: 'Ranking' },
]

function Heading({ children }: { children: string }) {
  return <h3 className="mt-6 mb-3 text-base font-semibold tracking-wide uppercase">{children}</h3>
}

function List({ pacts }: { pacts: PactListItem[] }) {
  return (
    <Stagger as="ul" className="flex flex-col gap-3">
      {pacts.map((pact) => (
        <PactRow key={pact.id} pact={pact} />
      ))}
    </Stagger>
  )
}

export function PactsPage() {
  const pacts = usePacts()
  const [segment, setSegment] = useState<Segment>('running')

  const all = pacts.data ?? []
  const running = all.filter((p) => !isArchived(p.status))
  const archive = all.filter((p) => isArchived(p.status))
  // what is waiting on you comes first: invitations, then everything else
  const needsYou = running.filter((p) => p.my.state === 'invited')
  const rest = running.filter((p) => p.my.state !== 'invited')

  return (
    <>
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-2xl font-semibold">Zakłady</h2>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="icon"
            aria-label="Rozliczenia"
            title="Rozliczenia"
            nativeButton={false}
            render={<Link to="/ledger" />}
          >
            <Wallet />
          </Button>
          <Button nativeButton={false} render={<Link to="/pacts/new" />}>
            <Plus /> Zakład
          </Button>
        </div>
      </div>

      <div className="mt-6 flex gap-2" role="group" aria-label="Widok">
        {SEGMENTS.map(({ key, label }) => (
          <Button
            key={key}
            variant={segment === key ? 'default' : 'outline'}
            className="flex-1"
            aria-pressed={segment === key}
            onClick={() => setSegment(key)}
          >
            {label}
          </Button>
        ))}
      </div>

      {segment === 'ranking' ? (
        <div className="mt-6">
          <Ranking />
        </div>
      ) : (
        <div>
          {pacts.isError && (
            <div className="mt-6 flex flex-col items-start gap-2">
              <span role="alert" className="text-base text-destructive">
                Nie udało się wczytać zakładów.
              </span>
              <Button variant="outline" onClick={() => void pacts.refetch()}>
                Spróbuj ponownie
              </Button>
            </div>
          )}

          {pacts.isPending && (
            <div className="mt-6 flex flex-col gap-3" role="status" aria-label="Ładowanie zakładów">
              {[0, 1, 2].map((i) => (
                <Skeleton key={i} className="h-24" />
              ))}
            </div>
          )}

          {pacts.isSuccess && segment === 'running' && (
            <>
              {running.length === 0 && (
                <p className="mt-10 text-center text-base text-muted-foreground">
                  Brak trwających zakładów. Zaproponuj pierwszy!
                </p>
              )}
              {needsYou.length > 0 && (
                <section>
                  <Heading>Czekają na Twoją odpowiedź</Heading>
                  <List pacts={needsYou} />
                </section>
              )}
              {rest.length > 0 && (
                <section>
                  <Heading>
                    {needsYou.length > 0
                      ? 'Pozostałe'
                      : `${running.length} ${plural(running.length, PACT_FORMS)}`}
                  </Heading>
                  <List pacts={rest} />
                </section>
              )}
            </>
          )}

          {pacts.isSuccess && segment === 'archive' && (
            <section>
              {archive.length === 0 ? (
                <p className="mt-10 text-center text-base text-muted-foreground">
                  Archiwum jest puste.
                </p>
              ) : (
                <>
                  <Heading>{`${archive.length} ${plural(archive.length, PACT_FORMS)}`}</Heading>
                  <List pacts={archive} />
                </>
              )}
            </section>
          )}
        </div>
      )}
    </>
  )
}
