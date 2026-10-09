import type { SharedKindKey } from '@/lib/api-types'

/**
 * How an expense and an income read. They are the same thing with the sign flipped (an income's
 * "payers" received the money, which belongs to the people it is for), so the form, the detail
 * page and the feed share one shape and only the words differ.
 */
export interface SharedWording {
  /** Chips and headings: "Wydatek". */
  noun: string
  /** "Nowy wydatek", "Edycja wydatku". */
  newHeading: string
  editHeading: string
  /** The title field's name and its hint. */
  titleLabel: string
  titlePlaceholder: string
  /** The payers' section and what one of them did. */
  payersHeading: string
  /** "zapłacił(a)", for "ann zapłacił(a) 40 zł". */
  paid: string
  /** "zapłaciłeś(-aś) Ty". */
  paidMe: string
  /** "zapłacili: ann, ben". */
  paidMany: string
  /** The column with each person's part. */
  partHeading: string
  submit: string
  cancel: string
  cancelQuestion: string
  /** Local validation. */
  titleMissing: string
  payersMismatch: (total: string, paid: string) => string
}

export const WORDING: Record<SharedKindKey, SharedWording> = {
  expense: {
    noun: 'Wydatek',
    newHeading: 'Nowy wydatek',
    editHeading: 'Edycja wydatku',
    titleLabel: 'Za co',
    titlePlaceholder: 'Za co? np. Pizza, nocleg',
    payersHeading: 'Kto zapłacił',
    paid: 'zapłacił(a)',
    paidMe: 'zapłaciłeś(-aś) Ty',
    paidMany: 'zapłacili',
    partHeading: 'Podział',
    submit: 'Dodaj wydatek',
    cancel: 'Usuń wydatek',
    cancelQuestion: 'Usunąć ten wydatek? Przestanie się liczyć w bilansie.',
    titleMissing: 'Podaj, za co był wydatek.',
    payersMismatch: (total, paid) => `Płacący muszą razem wyłożyć ${total}, a jest ${paid}.`,
  },
  income: {
    noun: 'Przychód',
    newHeading: 'Nowy przychód',
    editHeading: 'Edycja przychodu',
    titleLabel: 'Skąd',
    titlePlaceholder: 'Skąd? np. zwrot kaucji',
    payersHeading: 'Kto otrzymał',
    paid: 'otrzymał(a)',
    paidMe: 'otrzymałeś(-aś) Ty',
    paidMany: 'otrzymali',
    partHeading: 'Komu się należy',
    submit: 'Dodaj przychód',
    cancel: 'Usuń przychód',
    cancelQuestion: 'Usunąć ten przychód? Przestanie się liczyć w bilansie.',
    titleMissing: 'Podaj, skąd są pieniądze.',
    payersMismatch: (total, paid) => `Otrzymujący muszą razem dostać ${total}, a jest ${paid}.`,
  },
}
