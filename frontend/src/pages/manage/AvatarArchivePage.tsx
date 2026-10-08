// TEMPORARY: restores pictures of profile-picture votes closed before they were kept.
// Remove together with the avatar-archive endpoints in backend/voting.
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useRef, type ChangeEvent } from 'react'

import { BackLink } from '@/components/layout/BackLink'
import { Button } from '@/components/ui/button'
import { validateImage } from '@/features/admin/avatar'
import { UserAvatar } from '@/features/auth/UserAvatar'
import { apiFetch } from '@/lib/api'
import { formErrors } from '@/lib/api-errors'
import type { PollDetail } from '@/lib/api-types'

const key = ['admin', 'avatar-archive'] as const

function useArchive() {
  return useQuery({
    queryKey: key,
    queryFn: () => apiFetch<PollDetail[]>('/api/polls/avatar-archive/'),
  })
}

function useUploadPicture() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({
      id,
      field,
      file,
    }: {
      id: number
      field: 'previous' | 'proposed'
      file: File
    }) => {
      const form = new FormData()
      form.append(field, file)
      return apiFetch<PollDetail>(`/api/polls/${id}/archive-pictures/`, { method: 'PUT', form })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: key }),
  })
}

function Slot({
  label,
  username,
  src,
  disabled,
  onFile,
}: {
  label: string
  username: string
  src: string | null
  disabled: boolean
  onFile: (file: File) => void
}) {
  const input = useRef<HTMLInputElement>(null)
  const onChange = (e: ChangeEvent<HTMLInputElement>) => {
    const chosen = e.target.files?.[0]
    e.target.value = ''
    if (!chosen) return
    const problem = validateImage(chosen)
    if (problem) return window.alert(problem)
    onFile(chosen)
  }
  return (
    <div className="flex flex-col items-center gap-2">
      <span className="text-sm text-muted-foreground">{label}</span>
      <UserAvatar username={username} src={src} size="xl" />
      <input
        ref={input}
        type="file"
        accept="image/*"
        className="hidden"
        aria-label={label}
        onChange={onChange}
      />
      <Button
        type="button"
        variant="outline"
        size="sm"
        disabled={disabled}
        onClick={() => input.current?.click()}
      >
        {src ? 'Zamień' : 'Wgraj'}
      </Button>
    </div>
  )
}

export function AvatarArchivePage() {
  const { data, isPending, error } = useArchive()
  const upload = useUploadPicture()
  const uploadError = upload.isError ? formErrors(upload.error) : null
  return (
    <>
      <BackLink to="/manage">Zarządzanie</BackLink>
      <h2 className="mb-2 text-xl font-semibold">Zdjęcia z głosowań</h2>
      <p className="mb-6 text-base text-muted-foreground">
        Tymczasowe: uzupełnianie zdjęć w zakończonych głosowaniach o zmianę zdjęcia.
      </p>
      {isPending && <span className="text-sm text-muted-foreground">Ładowanie…</span>}
      {error && (
        <span role="alert" className="text-sm text-destructive">
          Nie udało się wczytać głosowań.
        </span>
      )}
      {uploadError && (
        <span role="alert" className="mb-4 block text-sm text-destructive">
          {uploadError.fields.previous ??
            uploadError.fields.proposed ??
            uploadError.general ??
            'Nie udało się wgrać zdjęcia.'}
        </span>
      )}
      <ul className="flex flex-col gap-3">
        {data?.map((poll) => {
          const username = String(poll.config.target_username ?? '')
          const removing = poll.config.remove === true
          return (
            <li key={poll.id} className="flex flex-col gap-4 rounded-lg border bg-card p-4">
              <div>
                <div className="text-lg font-medium">{poll.title}</div>
                <div className="text-sm text-muted-foreground">
                  {new Date(poll.created_at).toLocaleDateString('pl-PL')} ·{' '}
                  {poll.result?.applied === true
                    ? 'zastosowano'
                    : poll.result?.approved === true
                      ? 'przyjęto, nie zastosowano'
                      : 'nie przyjęto'}
                </div>
              </div>
              <div className="flex items-start justify-around gap-4">
                <Slot
                  label="Przed"
                  username={username}
                  src={poll.previous_avatar_url}
                  disabled={upload.isPending}
                  onFile={(file) => upload.mutate({ id: poll.id, field: 'previous', file })}
                />
                {removing ? (
                  <span className="self-center text-base font-semibold">usunięcie zdjęcia</span>
                ) : (
                  <Slot
                    label="Po"
                    username={username}
                    src={poll.proposed_avatar_url}
                    disabled={upload.isPending}
                    onFile={(file) => upload.mutate({ id: poll.id, field: 'proposed', file })}
                  />
                )}
              </div>
            </li>
          )
        })}
      </ul>
      {data?.length === 0 && (
        <span className="text-base text-muted-foreground">Brak zakończonych głosowań.</span>
      )}
    </>
  )
}
