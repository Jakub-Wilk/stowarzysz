const rules = new Intl.PluralRules('pl')

/** Polish noun forms: 1 głos, 2 głosy, 5 głosów. */
export interface PluralForms {
  one: string
  few: string
  many: string
}

/** Picks the Polish noun form for `count` (e.g. `plural(3, forms)` -> `few`). */
export function plural(count: number, forms: PluralForms): string {
  const category = rules.select(count)
  return category === 'one' ? forms.one : category === 'few' ? forms.few : forms.many
}

export const VOTE_FORMS: PluralForms = { one: 'głos', few: 'głosy', many: 'głosów' }
export const DAY_FORMS: PluralForms = { one: 'dzień', few: 'dni', many: 'dni' }
export const PARTICIPANT_FORMS: PluralForms = {
  one: 'uczestnik',
  few: 'uczestników',
  many: 'uczestników',
}
export const PACT_FORMS: PluralForms = { one: 'zakład', few: 'zakłady', many: 'zakładów' }
export const PICTURE_FORMS: PluralForms = { one: 'zdjęcie', few: 'zdjęcia', many: 'zdjęć' }
export const PERSON_FORMS: PluralForms = { one: 'osoba', few: 'osoby', many: 'osób' }
export const ITEM_FORMS: PluralForms = { one: 'pozycja', few: 'pozycje', many: 'pozycji' }
