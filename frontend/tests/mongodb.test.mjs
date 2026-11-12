import assert from 'node:assert/strict'
import { test } from 'node:test'
import { mongoFind } from '../app/utils/mongoQuery.ts'
import { parseConnectionUrl } from '../app/utils/connectionUrl.ts'
import { providerOptions } from '../app/providers/index.ts'
import { connectionKey } from '../app/services/connectionPreferences.ts'

test('MongoDB preference identities distinguish authentication databases', async () => {
  const input = parseConnectionUrl('mongodb://reader@localhost/catalog')
  assert.notEqual(await connectionKey(input), await connectionKey({ ...input, authSource: 'admin' }))
})

test('MongoDB find keeps nested filters, BSON values, sorting and projection', () => {
  const result = JSON.parse(mongoFind('orders', '{"_id":{"$oid":"507f1f77bcf86cd799439011"},"amount":{"$numberLong":"9007199254740993"}}', '{"createdAt":-1}', '{"name":1}'))
  assert.equal(result.collection, 'orders')
  assert.equal(result.filter.amount.$numberLong, '9007199254740993')
  assert.deepEqual(result.sort, { createdAt: -1 })
  assert.deepEqual(result.projection, { name: 1 })
  for (const value of ['[]', 'null', 'true', '{broken', '{"n":9007199254740993}', '{"n":1e400}'])
    assert.throws(() => mongoFind('orders', value, '{}', '{}'))
  assert.throws(() => mongoFind('', '{}', '{}', '{}'))
})

test('MongoDB URL import preserves credentials and authSource without accepting advanced options', () => {
  const input = parseConnectionUrl('mongodb://reader:p%40ss@localhost:27018/catalog?authSource=admin&tls=true')
  assert.equal(input.provider, 'mongodb')
  assert.equal(input.port, 27018)
  assert.equal(input.password, 'p@ss')
  assert.equal(input.authSource, 'admin')
  assert.equal(input.ssl, true)
  assert.equal(input.readOnly, true)
  assert.equal(parseConnectionUrl('mongodb://localhost/catalog').authSource, 'catalog')
  assert.equal(parseConnectionUrl('mongodb://localhost').allDatabases, true)
  for (const url of ['mongodb+srv://host/catalog', 'mongodb://a,b/catalog', 'mongodb://localhost/catalog?tlsAllowInvalidCertificates=true', 'mongodb://localhost/catalog?authSource=a&authSource=b'])
    assert.throws(() => parseConnectionUrl(url))
  assert.equal(providerOptions('mongodb').editing, false)
  assert.equal(providerOptions('mongodb').port, 27017)
})
