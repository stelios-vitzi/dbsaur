import assert from 'node:assert/strict'
import { test } from 'node:test'
import { loadMapPreferences, saveMapPreferences, validateMapUrl } from '../app/utils/mapPreferences.ts'

test('map requests are opt-in and storage failure defaults offline', () => {
  let value = null
  globalThis.localStorage = { getItem: () => value, setItem: (_, next) => { value = next } }
  assert.equal(loadMapPreferences().enabled, false)
  saveMapPreferences({ enabled: true, url: 'http://127.0.0.1:8080/{z}/{x}/{y}.png', attribution: 'Local tiles' })
  assert.equal(loadMapPreferences().enabled, true)
  assert.throws(() => saveMapPreferences({ enabled: true, url: '', attribution: '' }))
  for (const url of ['javascript:alert(1)', 'file:///tmp/map', 'https://user:password@tiles.example/style.json'])
    assert.throws(() => validateMapUrl(url))
  globalThis.localStorage = { getItem: () => { throw new Error('blocked') } }
  assert.equal(loadMapPreferences().enabled, false)
  delete globalThis.localStorage
})
