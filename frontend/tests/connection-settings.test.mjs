import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import ts from 'typescript'
import * as vue from 'vue'
import { providerOptions } from '../app/providers/index.ts'
import { parseConnectionUrl } from '../app/utils/connectionUrl.ts'

function setup(t, component, initial) {
  const { descriptor } = parse(
    readFileSync(new URL(`../app/components/${component}.vue`, import.meta.url), 'utf8'),
  )
  const compiled = compileScript(descriptor, { id: 'connection-settings-test' })
  const javascript = ts.transpileModule(compiled.content, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText
  const module = { exports: {} }
  new Function('require', 'module', 'exports', javascript)(
    (name) => {
      if (name === 'vue')
        return {
          ...vue,
          onMounted() {},
          useId: () => 'test-form',
          useTemplateRef: () => vue.ref({ reportValidity: () => true }),
        }
      if (name.endsWith('/providers')) return { providerOptions }
      if (name.endsWith('/connectionUrl')) return { parseConnectionUrl }
      return {}
    },
    module,
    module.exports,
  )
  const props = vue.reactive(initial)
  const events = []
  const scope = vue.effectScope()
  t.after(() => scope.stop())
  const view = scope.run(() =>
    module.exports.default.setup(props, { expose() {}, emit: (...event) => events.push(event) }),
  )
  return { view, props, events }
}
const details = () => ({
  id: 'saved-one',
  name: 'Local MongoDB',
  hasPassword: true,
  hasTlsKey: false,
  input: {
    provider: 'mongodb',
    host: 'localhost',
    port: 27018,
    database: 'catalog',
    username: 'alice',
    password: '',
    ssl: false,
    readOnly: true,
    authSource: 'auth',
  },
})

test('editing loads connection settings accurately and only saves on explicit Save', async (t) => {
  const writes = []
  const { view, events } = setup(t, 'ConnectionForm', {
    initial: details(),
    busy: false,
    savingEnabled: true,
    saveChanges: async (update) => writes.push(update),
  })
  assert.equal(view.form.port, 27018)
  assert.equal(view.form.authSource, 'auth')
  assert.equal(view.form.readOnly, true)
  assert.equal(view.changes().keepPassword, true)
  view.form.host = 'new-host'
  await vue.nextTick()
  assert.equal(writes.length, 0)
  await view.submit()
  assert.equal(writes.length, 1)
  assert.equal(writes[0].input.host, 'new-host')
  assert.equal(writes[0].input.password, '')
  assert.equal(writes[0].name, 'Local MongoDB')
  assert.deepEqual(events, [['saved']])
})

test('secret retention is explicit and does not carry credentials across provider changes', async (t) => {
  const initial = details()
  initial.input = { ...initial.input, provider: 'postgres', ssl: true, tlsCert: 'certificate' }
  initial.hasTlsKey = true
  const { view, props, events } = setup(t, 'ConnectionForm', {
    initial,
    busy: false,
    savingEnabled: true,
  })
  assert.equal(view.valid(), true)
  assert.equal(view.changes().keepTlsKey, true)
  view.form.password = 'new-test-password'
  assert.equal(view.changes().keepPassword, false)
  view.form.password = ''
  view.clearPassword.value = true
  assert.equal(view.changes().keepPassword, false)
  view.removePem('tlsKey')
  assert.equal(view.changes().keepTlsKey, false)
  assert.equal(view.valid(), false)
  view.form.provider = 'mongodb'
  assert.equal(view.changes().keepTlsKey, false)
  assert.equal(view.changes().keepPassword, false)
  assert.equal(view.form.tlsCert, undefined)
  props.saveChanges = async () => {
    throw new Error('Save failed.')
  }
  await vue.nextTick()
  await view.submit()
  assert.equal(view.localError.value, 'Save failed.')
  assert.deepEqual(events, [])
})

test('Test connection uses retained credentials without saving or connecting', async (t) => {
  const tests = []
  const { view, events } = setup(t, 'ConnectionForm', {
    initial: details(),
    busy: false,
    savingEnabled: true,
    testChanges: async (update) => {
      tests.push(update)
      return { ok: true, elapsedMs: 4 }
    },
  })
  await view.test()
  assert.equal(tests.length, 1)
  assert.equal(tests[0].keepPassword, true)
  assert.deepEqual(events, [])
  assert.match(view.testResult.value, /successful/)
})

test('connection editor scopes loading, saving, testing and deletion to its connection', async (t) => {
  const deleted = []
  const updated = []
  const tested = []
  const { view, events } = setup(t, 'ConnectionSettingsDialog', {
    connectionId: 'saved-one',
    load: async () => details(),
    update: async (id, update) => updated.push([id, update]),
    testSaved: async (id, update) => {
      tested.push([id, update])
      return { ok: true, elapsedMs: 1 }
    },
    remove: async (id) => {
      deleted.push(id)
      return true
    },
  })
  await view.remove()
  assert.deepEqual(deleted, [])
  await view.load()
  assert.equal(view.selected.value.name, 'Local MongoDB')
  assert.deepEqual(deleted, [])
  const change = { name: 'Renamed', input: details().input, keepPassword: true, keepTlsKey: false }
  await view.save(change)
  await view.testChanges(change)
  assert.equal(updated[0][0], 'saved-one')
  assert.equal(tested[0][0], 'saved-one')
  await view.remove()
  assert.deepEqual(deleted, [])
  view.confirmingDelete.value = true
  await view.remove()
  assert.deepEqual(deleted, ['saved-one'])
  assert.deepEqual(events, [['close']])
})

test('connection editor refuses settings returned for a different connection', async (t) => {
  const { view } = setup(t, 'ConnectionSettingsDialog', {
    connectionId: 'saved-two',
    load: async () => details(),
  })
  await view.load()
  assert.equal(view.selected.value, null)
  assert.match(view.error.value, /Reopen its settings/)
})
