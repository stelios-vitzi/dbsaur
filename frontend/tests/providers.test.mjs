import assert from 'node:assert/strict'
import { test } from 'node:test'
import { MySQL, MSSQL, PostgreSQL, SQLite, sql } from '@codemirror/lang-sql'
import { providerOptions, quoteIdentifier } from '../app/providers/index.ts'

test('providers select their ports, dialects and safe identifier quoting', () => {
  for (const provider of ['mysql', 'mariadb']) {
    assert.equal(providerOptions(provider).port, 3306)
    assert.equal(providerOptions(provider).dialect, MySQL)
    assert.equal(quoteIdentifier('a`b', provider), '`a``b`')
  }
  assert.equal(providerOptions('sqlserver').port, 1433)
  assert.equal(providerOptions('sqlserver').dialect, MSSQL)
  assert.equal(quoteIdentifier('a]b', 'sqlserver'), '[a]]b]')
  assert.equal(providerOptions('sqlite').dialect, SQLite)
  assert.equal(providerOptions('sqlite').port, 0)
  assert.equal(quoteIdentifier('a"b', 'sqlite'), '"a""b"')
  assert.equal(providerOptions('postgres').port, 5432)
  assert.equal(providerOptions('postgres').dialect, PostgreSQL)
  assert.equal(quoteIdentifier('a"b', 'postgres'), '"a""b"')
})

import { EditorState } from '@codemirror/state'
import { CompletionContext } from '@codemirror/autocomplete'
import { databaseCompletion, providerSnippets } from '../app/utils/sqlCompletion.ts'

test('SQL completion uses the connected provider dialect', async () => {
  for (const provider of ['mysql', 'mariadb', 'sqlserver', 'sqlite']) {
    const table = { schema: provider === 'sqlserver' ? 'dbo' : 'example', name: 'Order Details', kind: 'BASE TABLE', columns: ['id'] }
    const doc = 'SELECT * FROM '
    const context = new CompletionContext(EditorState.create({ doc, extensions: [sql({ dialect: providerOptions(provider).dialect })] }), doc.length, true)
    const result = await databaseCompletion([table], table, provider)(context)
    assert.equal(result.options.find((option) => option.label === table.name)?.apply, quoteIdentifier(table.name, provider))
    const snippetContext = new CompletionContext(EditorState.create({ doc: '', extensions: [sql({ dialect: providerOptions(provider).dialect })] }), 0, true)
    const snippets = await providerSnippets(provider)(snippetContext)
    assert.ok(snippets.options.some((option) => option.label === (provider === 'sqlserver' ? 'TOP' : 'LIMIT')))
    if (provider === 'sqlserver') assert.ok(!snippets.options.some((option) => option.label === 'LIMIT'))
  }
})
