import { ofetch } from 'ofetch'

// Never retry database operations implicitly, including read queries with side effects.
export const $fetch = ofetch.create({ retry: 0, credentials: 'same-origin' })
