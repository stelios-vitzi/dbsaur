import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parseConnectionUrl } from '../app/utils/connectionUrl.ts'

test('imports credentials, IPv6, provider ports, and explicit TLS settings', () => {
  const input = parseConnectionUrl('postgresql://a%40b:p%3A%2F%25%40@[::1]:5544/my%20db?sslmode=verify-full')
  assert.deepEqual(input, { provider: 'postgres', host: '::1', port: 5544, database: 'my db', username: 'a@b', password: 'p:/%@', ssl: true, readOnly: false, allDatabases: false })
  for (const [scheme, provider, port] of [['postgres', 'postgres', 5432], ['mysql', 'mysql', 3306], ['mariadb', 'mariadb', 3306], ['sqlserver', 'sqlserver', 1433]]) {
    const parsed = parseConnectionUrl(`${scheme}://user:secret@db.example`)
    assert.equal(parsed.provider, provider)
    assert.equal(parsed.port, port)
    assert.equal(parsed.ssl, true)
    assert.equal(parsed.allDatabases, true)
  }
  assert.equal(parseConnectionUrl('mysql://user@localhost/db?ssl-mode=DISABLED').ssl, false)
})

test('rejects unsupported options and unsafe or malformed URLs without leaking secrets', () => {
  for (const url of [
    'https://user:secret@host/db',
    'postgres://user:secret@host/db?sslmode=prefer',
    'postgres://user:secret@host/db?sslmode=require',
    'postgres://user:secret@host/db?sslrootcert=/etc/ca.pem',
    'postgres://user:secret@host/db?ssl=true&ssl=false',
    'postgres://user:secret@host/db?ssl=true&encrypt=true',
    'postgres://user:secret@host/db#fragment',
    'postgres://user:secret@host/db/extra',
    'postgres://user:secret@host/%00',
    'postgres://user:secret@host/%ZZ',
    'postgres://user:secret@host:65536/db',
    'postgres://user:secret@host:0/db',
    'postgres://user:secret@ho\nst/db',
    'postgres://host/db',
    'postgres://user:secret@/db',
  ]) {
    assert.throws(() => parseConnectionUrl(url), (error) => {
      assert.ok(!error.message.includes('secret'))
      return true
    })
  }
})
