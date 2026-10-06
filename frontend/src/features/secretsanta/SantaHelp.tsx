import { Lightbulb, Plus, X } from 'lucide-react'
import { useState, type SubmitEvent } from 'react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { formatAmount } from '@/features/secretsanta/format'
import { useAnswerHelp, useAskForHelp } from '@/features/secretsanta/hooks'
import { formErrors } from '@/lib/api-errors'
import type { SantaGiverHelp, SantaReceiverHelp } from '@/lib/api-types'

const MAX_IDEAS = 5
const MAX_IDEA_LENGTH = 200

function IdeaList({ ideas }: { ideas: string[] }) {
  return (
    <ul className="flex list-disc flex-col gap-1 pl-5 text-left text-base">
      {ideas.map((idea, i) => (
        <li key={i}>{idea}</li>
      ))}
    </ul>
  )
}

/** Under a revealed victim: ask them, anonymously, for gift ideas and read the answer. */
export function GiverHelp({ victimId, help }: { victimId: number; help?: SantaGiverHelp }) {
  const ask = useAskForHelp()
  const error = ask.error ? formErrors(ask.error).general : null

  const askButton = (label: string, variant: 'outline' | 'ghost') => (
    <Button
      type="button"
      variant={variant}
      size="sm"
      disabled={ask.isPending}
      onClick={() => ask.mutate(victimId)}
    >
      <Lightbulb aria-hidden />
      {label}
    </Button>
  )

  return (
    <div className="flex w-full max-w-xs flex-col items-center gap-2">
      {help && help.ideas.length > 0 && (
        <div className="animate-fade-up flex w-full flex-col gap-2 rounded-md border p-3">
          <span className="text-left text-sm text-muted-foreground">Podpowiedzi:</span>
          <IdeaList ideas={help.ideas} />
        </div>
      )}
      {help?.pending ? (
        <p className="text-sm text-muted-foreground">Czekamy na podpowiedzi…</p>
      ) : help ? (
        askButton('Poproś o więcej', 'ghost')
      ) : (
        <>
          {askButton('Nie wiem, co kupić', 'outline')}
          <p className="text-xs text-muted-foreground">
            Poprosimy o kilka pomysłów, nie zdradzając, kto pyta.
          </p>
        </>
      )}
      {error && (
        <span role="alert" className="text-sm text-destructive">
          {error}
        </span>
      )}
    </div>
  )
}

function AnswerForm({ request, onDone }: { request: SantaReceiverHelp; onDone: () => void }) {
  const save = useAnswerHelp()
  const [ideas, setIdeas] = useState<string[]>(request.ideas.length ? request.ideas : ['', ''])
  const errors = formErrors(save.error)
  const error = save.error ? (errors.fields.ideas ?? errors.general) : null
  const filled = ideas.some((idea) => idea.trim())

  const change = (index: number, value: string) =>
    setIdeas((current) => current.map((idea, i) => (i === index ? value : idea)))

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!filled || save.isPending) return
    save.mutate(
      { id: request.id, ideas: ideas.map((idea) => idea.trim()).filter(Boolean) },
      { onSuccess: onDone },
    )
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-2">
      {ideas.map((idea, i) => (
        // Rows have no identity beyond their position.
        <div key={i} className="flex items-center gap-2">
          <Input
            value={idea}
            maxLength={MAX_IDEA_LENGTH}
            placeholder={i === 0 ? 'np. książka kucharska' : 'Kolejny pomysł'}
            aria-label={`Pomysł ${i + 1}`}
            onChange={(e) => change(i, e.target.value)}
          />
          {ideas.length > 1 && (
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label={`Usuń pomysł ${i + 1}`}
              onClick={() => setIdeas((current) => current.filter((_, j) => j !== i))}
            >
              <X aria-hidden />
            </Button>
          )}
        </div>
      ))}
      {ideas.length < MAX_IDEAS && (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="self-start"
          onClick={() => setIdeas((current) => [...current, ''])}
        >
          <Plus aria-hidden />
          Dodaj pomysł
        </Button>
      )}
      {error && (
        <span role="alert" className="text-sm text-destructive">
          {error}
        </span>
      )}
      <Button type="submit" disabled={!filled || save.isPending}>
        Wyślij pomysły
      </Button>
    </form>
  )
}

function RequestCard({ request }: { request: SantaReceiverHelp }) {
  const [editing, setEditing] = useState(false)
  const amount = request.amount === null ? '' : ` za ${formatAmount(request.amount)}`

  return (
    <li className="flex flex-col gap-3 rounded-lg border bg-card p-4">
      <div className="flex items-start gap-3">
        <Lightbulb className="mt-0.5 size-6 shrink-0" aria-hidden />
        <div className="flex flex-col gap-1">
          <span className="text-base font-semibold">
            {request.pending ? 'Twój Secret Santa prosi o pomoc' : 'Pomysły wysłane'}
          </span>
          <span className="text-sm text-muted-foreground">
            {request.pending
              ? `Ktoś nie wie, co Ci kupić, i potrzebuje pomocy z prezentem${amount}. ` +
                'Podsuń kilka pomysłów. Nie dowiesz się, kto pyta.'
              : `Twoje podpowiedzi do prezentu${amount}:`}
          </span>
        </div>
      </div>
      {request.pending || editing ? (
        <AnswerForm request={request} onDone={() => setEditing(false)} />
      ) : (
        <>
          <IdeaList ideas={request.ideas} />
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="self-start"
            onClick={() => setEditing(true)}
          >
            Edytuj
          </Button>
        </>
      )}
    </li>
  )
}

/** Anonymous requests for ideas about the signed-in user, pending ones first. */
export function HelpRequestsForMe({ requests }: { requests: SantaReceiverHelp[] }) {
  if (requests.length === 0) return null
  const ordered = [...requests].sort((a, b) => Number(b.pending) - Number(a.pending))
  return (
    <ul className="flex flex-col gap-3">
      {ordered.map((request) => (
        <RequestCard key={request.id} request={request} />
      ))}
    </ul>
  )
}
