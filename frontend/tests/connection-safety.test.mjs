import assert from 'node:assert/strict'
import { test } from 'node:test'
import { connectionSafety } from '../app/utils/connectionSafety.ts'
import { parseConnectionUrl } from '../app/utils/connectionUrl.ts'

test('only explicit loopbacks get development defaults', () => {
  for (const host of ['localhost', 'LOCALHOST', '127.0.0.1', '::1', '[::1]'])
    assert.deepEqual(connectionSafety(host), { ssl: false, readOnly: false })
  for (const host of ['db.example', 'localhost.example', '10.0.0.1', '192.168.1.1', '::ffff:127.0.0.1', ''])
    assert.deepEqual(connectionSafety(host), { ssl: true, readOnly: true })
  assert.deepEqual(connectionSafety('', 'sqlite'), { ssl: false, readOnly: false })
})

test('import defaults to locked verified TLS; explicit downgrade preserves lock', () => {
  assert.equal(parseConnectionUrl('postgres://user@db.example/db').ssl, true)
  const downgraded = parseConnectionUrl('postgres://user@db.example/db?sslmode=disable')
  assert.equal(downgraded.ssl, false)
  assert.equal(downgraded.readOnly, true)
})
