import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useRef, useState, type ChangeEvent } from 'react'

import { BackLink } from '@/components/layout/BackLink'
import { SelectField } from '@/components/SelectField'
import { Button } from '@/components/ui/button'
import { ledgerKey } from '@/features/ledger/hooks'
import { ConfirmDialog } from '@/features/voting/ConfirmDialog'
import { usePeople } from '@/features/voting/hooks'
import { apiFetch } from '@/lib/api'
import { formErrors } from '@/lib/api-errors'
import type { TricountPreview, TricountResult } from '@/lib/api-types'
import { ENTRY_FORMS, plural } from '@/lib/plural'

const MAX_BYTES = 10 * 1024 * 1024

type Mapping = Record<string, number | null>

function errorText(error: unknown, fallback: string): string {
  const { fields, general } = formErrors(error)
  return fields.dump ?? fields.mapping ?? general ?? fallback
}

export function TricountImportPage() {
  const queryClient = useQueryClient()
  const { data: people } = usePeople()
  const input = useRef<HTMLInputElement>(null)
  const [dump, setDump] = useState<unknown>(null)
  const [mapping, setMapping] = useState<Mapping>({})
  const [confirming, setConfirming] = useState(false)
  const [readError, setReadError] = useState<string | null>(null)

  const preview = useMutation({
    mutationFn: (body: unknown) =>
      apiFetch<TricountPreview>('/api/ledger/import/preview/', {
        method: 'POST',
        json: { dump: body },
      }),
    onSuccess: (data) =>
      setMapping(Object.fromEntries(data.participants.map((p) => [p.name, p.user_id]))),
  })

  const run = useMutation({
    mutationFn: () =>
      apiFetch<TricountResult>('/api/ledger/import/', {
        method: 'POST',
        json: { dump, mapping },
      }),
    onSuccess: () => {
      setConfirming(false)
      void queryClient.invalidateQueries({ queryKey: ledgerKey })
    },
    onError: () => setConfirming(false),
  })

  const onFile = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    e.target.value = ''
    if (!file) return
    setReadError(null)
    run.reset()
    if (file.size > MAX_BYTES) return setReadError('Plik jest za duży (maksymalnie 10 MB).')
    try {
      const parsed: unknown = JSON.parse(await file.text())
      setDump(parsed)
      preview.mutate(parsed)
    } catch {
      setReadError('To nie jest poprawny plik JSON.')
    }
  }

  const data = preview.data
  const chosen = Object.values(mapping).filter((id): id is number => id !== null)
  const complete = data !== undefined && data.participants.every((p) => mapping[p.name] != null)
  const distinct = new Set(chosen).size === chosen.length
  const options = (people ?? []).map((p) => ({ value: String(p.id), label: p.username }))
  const done = run.data

  return (
    <>
      <BackLink to="/manage">Zarządzanie</BackLink>
      <h2 className="mb-2 text-xl font-semibold">Import z Tricount</h2>
      <p className="mb-6 text-base text-muted-foreground">
        Wgraj plik JSON zapisany przez tricount-exporter z opcją <code>--save-response</code>.
        Wydatki, przychody i spłaty trafią do Rozliczeń od razu, bez powiadomień. Ponowny import
        tego samego pliku pomija wpisy, które już są.
      </p>
      <input
        ref={input}
        type="file"
        accept="application/json,.json"
        className="hidden"
        aria-label="Plik z Tricount"
        onChange={onFile}
      />
      <Button type="button" variant="outline" onClick={() => input.current?.click()}>
        {data ? 'Wybierz inny plik' : 'Wybierz plik'}
      </Button>
      {(readError || preview.isError) && (
        <span role="alert" className="mt-4 block text-sm text-destructive">
          {readError ?? errorText(preview.error, 'Nie udało się odczytać pliku.')}
        </span>
      )}
      {preview.isPending && (
        <span className="mt-4 block text-sm text-muted-foreground">Czytanie pliku…</span>
      )}

      {data && !done && (
        <div className="mt-6 flex flex-col gap-6">
          <div className="rounded-lg border bg-card p-4">
            <div className="text-lg font-medium">{data.title}</div>
            <ul className="mt-2 text-sm text-muted-foreground">
              <li>
                Wydatki: {data.expenses} · przychody: {data.incomes} · spłaty: {data.payments}
              </li>
              {data.first_date && data.last_date && (
                <li>
                  Od {new Date(data.first_date).toLocaleDateString('pl-PL')} do{' '}
                  {new Date(data.last_date).toLocaleDateString('pl-PL')}
                  {data.currencies.length > 0 && ` · waluty: ${data.currencies.join(', ')}`}
                </li>
              )}
              {data.already_imported > 0 && (
                <li>
                  Już zaimportowane (zostaną pominięte): {data.already_imported}{' '}
                  {plural(data.already_imported, ENTRY_FORMS)}
                </li>
              )}
              {data.skipped_deleted > 0 && <li>Usunięte w Tricount: {data.skipped_deleted}</li>}
              {data.attachments > 0 && (
                <li>Zdjęcia z Tricount ({data.attachments}) nie są importowane.</li>
              )}
            </ul>
          </div>

          <div className="flex flex-col gap-3">
            <h3 className="text-base font-semibold">Kto jest kim</h3>
            {data.participants.map((p) => (
              <div key={p.name} className="grid grid-cols-2 items-center gap-3">
                <span className="text-base">{p.name}</span>
                <SelectField
                  aria-label={`Poseł dla ${p.name}`}
                  placeholder="Wybierz posła"
                  options={options}
                  value={mapping[p.name] != null ? String(mapping[p.name]) : null}
                  onChange={(value) => setMapping({ ...mapping, [p.name]: Number(value) })}
                />
              </div>
            ))}
            {!distinct && (
              <span role="alert" className="text-sm text-destructive">
                Każda osoba może być przypisana tylko raz.
              </span>
            )}
          </div>

          {run.isError && (
            <span role="alert" className="text-sm text-destructive">
              {errorText(run.error, 'Import się nie udał. Nic nie zostało zapisane.')}
            </span>
          )}
          <Button
            type="button"
            disabled={!complete || !distinct || run.isPending}
            onClick={() => setConfirming(true)}
          >
            Importuj
          </Button>
        </div>
      )}

      {done && (
        <div role="status" className="mt-6 rounded-lg border bg-card p-4 text-base">
          Zaimportowano {done.imported} {plural(done.imported, ENTRY_FORMS)}
          {done.skipped_existing > 0 && `, pominięto już istniejące: ${done.skipped_existing}`}
          {done.skipped_deleted > 0 && `, pominięto usunięte: ${done.skipped_deleted}`}.
        </div>
      )}

      <ConfirmDialog
        open={confirming}
        onOpenChange={setConfirming}
        title="Zaimportować wpisy?"
        description="Wpisy pojawią się w Rozliczeniach wszystkich posłów i zmienią bilans. Nikt nie dostanie powiadomienia."
        confirmLabel="Importuj"
        pending={run.isPending}
        onConfirm={() => run.mutate()}
      />
    </>
  )
}
