import { ApiError } from '@/lib/api'

export interface FormErrors {
  /** Messages keyed by form field name. */
  fields: Record<string, string>
  /** Errors not tied to a field (or an unknown failure). */
  general: string | null
}

const toText = (value: unknown): string =>
  Array.isArray(value) ? value.map(String).join(' ') : String(value)

/** Flattens a DRF error body ({field: [msgs]}, {detail}, or [msgs]) for display in a form. */
export function formErrors(error: unknown): FormErrors {
  const fallback = 'Coś poszło nie tak. Spróbuj ponownie.'
  if (!(error instanceof ApiError)) return { fields: {}, general: fallback }
  const body = error.body
  if (Array.isArray(body)) return { fields: {}, general: toText(body) }
  if (body && typeof body === 'object') {
    const fields: Record<string, string> = {}
    let general: string | null = null
    for (const [key, value] of Object.entries(body)) {
      if (key === 'detail' || key === 'non_field_errors') general = toText(value)
      else fields[key] = toText(value)
    }
    return { fields, general: general ?? (Object.keys(fields).length ? null : fallback) }
  }
  return { fields: {}, general: fallback }
}
