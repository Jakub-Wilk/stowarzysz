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
export const PACT_FORMS: PluralForms = { one: 'umowa', few: 'umowy', many: 'umów' }
export const PICTURE_FORMS: PluralForms = { one: 'zdjęcie', few: 'zdjęcia', many: 'zdjęć' }
export const PERSON_FORMS: PluralForms = { one: 'osoba', few: 'osoby', many: 'osób' }
export const ITEM_FORMS: PluralForms = { one: 'pozycja', few: 'pozycje', many: 'pozycji' }
/** After "dla": dla 1 osoby, dla 3 osób. */
export const PERSON_GENITIVE_FORMS: PluralForms = { one: 'osoby', few: 'osób', many: 'osób' }
export const ENTRY_FORMS: PluralForms = { one: 'wpis', few: 'wpisy', many: 'wpisów' }
