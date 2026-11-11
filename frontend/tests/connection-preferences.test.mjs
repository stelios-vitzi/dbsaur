import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  createConnectionPreferences,
  connectionKey,
} from '../app/services/connectionPreferences.ts'

test('connection preferences survive restart, remain isolated, and can be forgotten', () => {
  const values = new Map()
  const storage = () => ({
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value),
    removeItem: (key) => values.delete(key),
  })
  const first = createConnectionPreferences(storage)
  first.setActiveConnectionKey('saved:one')
  first.rememberDatabase('customers')
  first.rememberDatabase('orders', 'saved:two')
  const restarted = createConnectionPreferences(storage)
  restarted.restore()
  assert.equal(restarted.activeKey, 'saved:one')
  assert.equal(restarted.storedDatabase(restarted.activeKey), 'customers')
  assert.equal(restarted.storedDatabase('saved:two'), 'orders')
  restarted.forgetDatabase('saved:one')
  restarted.setActiveConnectionKey(undefined)
  first.restore()
  assert.equal(first.activeKey, undefined)
  assert.equal(first.storedDatabase('saved:one'), undefined)
  assert.equal(first.storedDatabase('saved:two'), 'orders')
})

test('denied browser storage does not interrupt connection preference operations', () => {
  const preferences = createConnectionPreferences(() => {
    throw new Error('denied')
  })
  preferences.setActiveConnectionKey('saved:one')
  preferences.rememberDatabase('customers')
  preferences.forgetDatabase('saved:one')
  assert.equal(preferences.activeKey, 'saved:one')
  assert.equal(preferences.storedDatabase('saved:one'), undefined)
  preferences.restore()
  assert.equal(preferences.activeKey, undefined)
})

test('unsaved connection keys are opaque, ignore secrets, and distinguish identities', async () => {
  const input = {
    provider: 'postgres',
    host: 'LOCALHOST',
    port: 5432,
    username: 'reader',
    password: 'first',
    database: 'one',
  }
  const key = await connectionKey(input)
  assert.match(key, /^connection:[a-f0-9]{64}$/)
  assert.equal(
    await connectionKey({ ...input, host: ' localhost ', password: 'rotated', database: 'two' }),
    key,
  )
  assert.notEqual(await connectionKey({ ...input, username: 'other' }), key)
  assert.equal(await connectionKey(input, 'profile'), 'saved:profile')
  assert.equal(await connectionKey({ ...input, provider: 'demo' }), undefined)
})
