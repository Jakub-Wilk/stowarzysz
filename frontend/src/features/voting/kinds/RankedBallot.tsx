import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type Announcements,
  type DragEndEvent,
} from '@dnd-kit/core'
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { GripVertical } from 'lucide-react'
import { useState } from 'react'

import { Button } from '@/components/ui/button'
import { readOptions, readPicks, type PollOption } from '@/features/voting/kinds/optionsData'
import { cn } from '@/lib/utils'

import type { BallotInputProps } from './types'

function SortableRow({ option, place }: { option: PollOption; place: number }) {
  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: option.id })

  return (
    <li
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={cn(
        'relative flex items-center gap-3 rounded-lg border bg-card px-3 py-3',
        isDragging && 'z-10 border-primary shadow-lg',
      )}
    >
      <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-muted text-base font-bold tabular-nums">
        {place}
      </span>
      <span className="text-metal flex-1 text-lg font-medium break-words">{option.label}</span>
      {/* only the handle starts a drag, so the rest of the row still scrolls the page */}
      <button
        type="button"
        ref={setActivatorNodeRef}
        aria-label={`Przeciągnij, aby zmienić miejsce: ${option.label}`}
        className="flex size-11 shrink-0 cursor-grab touch-none items-center justify-center rounded-md text-muted-foreground outline-none focus-visible:ring-3 focus-visible:ring-ring/50 active:cursor-grabbing"
        {...attributes}
        {...listeners}
      >
        <GripVertical className="size-6" aria-hidden />
      </button>
    </li>
  )
}

/** Opening order, overlaid with a saved ranking (options it leaves out go last). */
function initialOrder(options: PollOption[], saved: string[]): string[] {
  const ids = new Set(options.map((o) => o.id))
  const ranked = saved.filter((id) => ids.has(id))
  return [...ranked, ...options.map((o) => o.id).filter((id) => !ranked.includes(id))]
}

const sameOrder = (a: string[], b: string[]) =>
  a.length === b.length && a.every((x, i) => x === b[i])

export function RankedBallotInput({ poll, value, pending, onSubmit }: BallotInputProps) {
  const options = readOptions(poll.config)
  const labels = new Map(options.map((o) => [o.id, o.label]))
  const saved = readPicks(value)
  const [order, setOrder] = useState(() => initialOrder(options, saved))
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  const label = (id: string | number) => labels.get(String(id)) ?? String(id)
  const place = (id: string | number) => order.indexOf(String(id)) + 1
  const announcements: Announcements = {
    onDragStart: ({ active }) => `Podniesiono: ${label(active.id)}. Miejsce ${place(active.id)}.`,
    onDragOver: ({ active, over }) =>
      over ? `${label(active.id)} jest nad: ${label(over.id)}.` : undefined,
    onDragEnd: ({ active, over }) =>
      `Upuszczono: ${label(active.id)}. Miejsce ${over ? place(over.id) : place(active.id)}.`,
    onDragCancel: ({ active }) => `Anulowano przenoszenie: ${label(active.id)}.`,
  }

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    if (!over || active.id === over.id) return
    setOrder((current) =>
      arrayMove(current, current.indexOf(String(active.id)), current.indexOf(String(over.id))),
    )
  }

  const unchanged = saved.length > 0 && sameOrder(order, initialOrder(options, saved))

  return (
    <div className="flex flex-col gap-4">
      <p className="text-base text-muted-foreground">
        Przeciągnij opcje, by ustawić je od najlepszej (1) do najgorszej.
      </p>
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={onDragEnd}
        accessibility={{
          announcements,
          screenReaderInstructions: {
            draggable:
              'Aby zmienić miejsce opcji, naciśnij spację, strzałkami przesuń ją w górę lub w dół, a następnie ponownie spację. Escape anuluje przenoszenie.',
          },
        }}
      >
        <SortableContext items={order} strategy={verticalListSortingStrategy}>
          <ol className="flex flex-col gap-2">
            {order.map((id, index) => (
              <SortableRow key={id} option={{ id, label: label(id) }} place={index + 1} />
            ))}
          </ol>
        </SortableContext>
      </DndContext>
      <Button disabled={unchanged || pending} onClick={() => onSubmit({ ranking: order })}>
        {saved.length === 0 ? 'Oddaj głos' : 'Zmień głos'}
      </Button>
    </div>
  )
}
