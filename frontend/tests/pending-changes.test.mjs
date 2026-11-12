import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import ts from 'typescript'
import * as vue from 'vue'
import {
  createConnectionPreferences,
  connectionKey,
} from '../app/services/connectionPreferences.ts'

function evaluate(source, runtime, imports = {}) {
  const javascript = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText
  const module = { exports: {} }
  const bindings = Object.entries(runtime).filter(([name]) => /^[a-zA-Z_$][\w$]*$/.test(name))
  new Function('require', 'module', 'exports', ...bindings.map(([name]) => name), javascript)(
    (name) =>
      name === 'vue' ? { ...vue, ...runtime } : (imports[name] ?? runtime.imports?.[name]),
    module,
    module.exports,
    ...bindings.map(([, value]) => value),
  )
  return module.exports
}
function source(path) {
  return readFileSync(new URL(`../app/${path}`, import.meta.url), 'utf8')
}
function setup(t, respond) {
  const writes = []
  const scope = vue.effectScope()
  const runtime = {
    ...vue,
    onMounted: () => {},
    $fetch: async (url, options) => {
      writes.push({ url, ...options })
      if (respond) return respond(url, options)
      return { affectedRows: 1 }
    },
  }
  let toastState
  runtime.inject = () => toastState
  const { useToast, createToastState } = evaluate(source('composables/useToast.ts'), runtime)
  toastState = createToastState()
  runtime.useToast = useToast
  runtime.imports = {
    '../composables/useToast': { useToast },
    './useToast': { useToast },
    '../services/http': { $fetch: runtime.$fetch },
    '../services/connectionPreferences': { createConnectionPreferences, connectionKey },
  }
  const cancelled = evaluate(source('utils/databaseChange.ts'), runtime)
  const { useDatabase } = evaluate(source('composables/useDatabase.ts'), runtime, {
    '../utils/databaseChange': cancelled,
  })
  const db = scope.run(useDatabase)
  const host = scope.run(useToast)
  db.session.value = { database: 'sample', provider: 'postgres' }
  t.after(() => scope.stop())
  return { db, host, writes, scope, runtime, cancelled }
}
const update = () => ({
  action: 'update',
  schema: 'public',
  table: 'items',
  original: { id: '1' },
  values: { name: 'new' },
})

test('active connection settings track only the saved connection used to connect', async (t) => {
  const { db } = setup(t, (url) => {
    if (url === '/api/connections') return { enabled: true, connections: [] }
    if (url === '/api/tables') return []
    return {
      provider: 'postgres',
      host: 'localhost',
      database: 'app',
      allDatabases: false,
      readOnly: false,
    }
  })
  await db.connect('saved-one')
  assert.equal(db.activeSavedConnectionId.value, 'saved-one')
  await db.connect('saved-two')
  assert.equal(db.activeSavedConnectionId.value, 'saved-two')
  await db.deleteSavedConnection('saved-one')
  assert.equal(db.activeSavedConnectionId.value, 'saved-two')
  await db.disconnect()
  assert.equal(db.activeSavedConnectionId.value, undefined)
  await db.connect({
    provider: 'demo',
    host: '',
    port: 0,
    database: '',
    username: '',
    password: '',
    ssl: false,
  })
  assert.equal(db.activeSavedConnectionId.value, undefined)
})

test('Save sends the staged snapshot once; no write happens before confirmation', async (t) => {
  const { db, host, writes } = setup(t)
  const mutation = vue.reactive(update())
  const result = db.applyMutation(mutation)
  assert.equal(writes.length, 0)
  assert.equal(host.messages.value[0].confirmation, true)
  mutation.values.name = 'changed later'
  const id = host.messages.value[0].id
  host.respond(id, true)
  host.respond(id, true)
  assert.deepEqual(await result, { affectedRows: 1 })
  assert.equal(writes.length, 1)
  assert.equal(writes[0].body.mutation.values.name, 'new')
  assert.equal(host.messages.value.length, 0)
})

test('Cancel discards a mutation without writing or reporting success', async (t) => {
  const { db, host, writes, cancelled } = setup(t)
  const result = db.applyMutation(update())
  const rejected = assert.rejects(result, cancelled.DatabaseChangeCancelled)
  host.respond(host.messages.value[0].id, false)
  await rejected
  assert.equal(writes.length, 0)
  assert.equal(host.messages.value.length, 0)
})

test('column deletion and definition snapshots require an explicit save', async (t) => {
  const { db, host, writes } = setup(t)
  const mutation = {
    action: 'alterColumn',
    schema: 'public',
    table: 'items',
    column: 'name',
    expected: {
      dataType: 'text',
      nullable: true,
      defaultExpression: null,
      characterSet: 'UTF8',
      collation: null,
      comment: '',
      extra: '',
      defaultConstraint: null,
    },
    change: { kind: 'drop' },
  }
  const result = db.applyMutation(mutation)
  assert.equal(writes.length, 0)
  mutation.expected.dataType = 'integer'
  mutation.change.kind = 'definition'
  host.respond(host.messages.value[0].id, true)
  await result
  assert.equal(writes.length, 1)
  assert.equal(writes[0].body.mutation.expected.dataType, 'text')
  assert.deepEqual(writes[0].body.mutation.change, { kind: 'drop' })
})

test('connection changes and owner disposal cancel pending writes', async (t) => {
  const { db, host, writes, scope, cancelled } = setup(t)
  for (const cancel of [
    () => {
      db.switching.value = true
    },
    () => scope.stop(),
  ]) {
    db.switching.value = false
    const rejected = assert.rejects(db.applyMutation(update()), cancelled.DatabaseChangeCancelled)
    cancel()
    await rejected
    assert.equal(host.messages.value.length, 0)
  }
  assert.equal(writes.length, 0)
})

test('pending confirmations survive notification overflow and never get dismissal timers', async (t) => {
  const { host, runtime, scope } = setup(t)
  const confirmation = host.confirm('Unsaved database change')
  const id = host.messages.value[0].id
  for (let index = 0; index < 10; index++) host.show(`Notice ${index}`)
  assert.ok(host.messages.value.some((item) => item.id === id))
  const scheduled = []
  const { descriptor } = parse(source('components/ToastHost.vue'))
  const compiled = compileScript(descriptor, { id: 'pending-toast-test' })
  const component = evaluate(compiled.content, {
    ...runtime,
    useTemplateRef: () => vue.ref(null),
    onBeforeUnmount: () => {},
    setTimeout: (callback) => {
      scheduled.push(callback)
      return scheduled.length
    },
    clearTimeout: () => {},
  }).default
  const hostScope = scope.run(() => vue.effectScope())
  const view = hostScope.run(() => component.setup({}, { expose() {} }))
  assert.equal(scheduled.length, 5)
  view.resume(id)
  assert.equal(scheduled.length, 5)
  for (const callback of scheduled) callback()
  await vue.nextTick()
  assert.deepEqual(
    host.messages.value.map((item) => item.id),
    [id],
  )
  host.respond(id, false)
  assert.equal(await confirmation, false)
})
