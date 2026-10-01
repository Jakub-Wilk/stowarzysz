import { Navigate, useParams } from 'react-router'

import { Button } from '@/components/ui/button'
import { useManagedUser } from '@/features/admin/hooks'
import { UserForm } from '@/features/admin/UserForm'
import { ApiError } from '@/lib/api'
import { BackLink } from '@/components/layout/BackLink'

function EditUser({ id }: { id: number }) {
  const { data: user, isPending, error, refetch } = useManagedUser(id)

  if (error instanceof ApiError && error.status === 404) {
    return <Navigate to="/manage/users" replace />
  }
  if (isPending) return <span className="text-sm text-muted-foreground">Ładowanie…</span>
  if (error) {
    return (
      <div className="flex flex-col items-start gap-2">
        <span role="alert" className="text-sm text-destructive">
          Nie udało się wczytać użytkownika.
        </span>
        <Button variant="outline" onClick={() => refetch()}>
          Spróbuj ponownie
        </Button>
      </div>
    )
  }
  return (
    <>
      <h2 className="mb-6 text-xl font-semibold">{user.username}</h2>
      <UserForm key={user.id} user={user} />
    </>
  )
}

/** `/manage/users/new` (no id) or `/manage/users/:id`. */
export function UserFormPage() {
  const { id } = useParams()
  const userId = id === undefined ? null : Number(id)
  if (userId !== null && !Number.isInteger(userId)) return <Navigate to="/manage/users" replace />

  return (
    <>
      <BackLink to="/manage/users">Użytkownicy</BackLink>
      {userId === null ? (
        <>
          <h2 className="mb-6 text-xl font-semibold">Nowy użytkownik</h2>
          <UserForm />
        </>
      ) : (
        <EditUser id={userId} />
      )}
    </>
  )
}
