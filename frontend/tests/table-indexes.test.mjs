import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import ts from 'typescript'
import * as vue from 'vue'
import { DatabaseChangeCancelled } from '../app/utils/databaseChange.ts'

function setup(t, apply) {
  const events = []
  const { descriptor } = parse(
    readFileSync(new URL('../app/components/TableIndexes.vue', import.meta.url), 'utf8'),
  )
  const compiled = compileScript(descriptor, { id: 'table-indexes-test' })
  const javascript = ts.transpileModule(compiled.content, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText
  const module = { exports: {} }
  new Function('require', 'module', 'exports', javascript)(
    (name) => {
      if (name === 'vue') return { ...vue, onMounted() {} }
      if (name.endsWith('databaseChange')) return { DatabaseChangeCancelled }
      if (name.endsWith('providers')) return { providerOptions: () => ({ nameLimit: 63 }) }
      return {}
    },
    module,
    module.exports,
  )
  const scope = vue.effectScope()
  t.after(() => scope.stop())
  const props = vue.reactive({
    table: { schema: 'public', name: 'items', columns: ['id', 'name'] },
    provider: 'postgres',
    disabled: false,
    apply,
  })
  const view = scope.run(() =>
    module.exports.default.setup(props, {
      expose() {},
      emit: (...event) => events.push(event),
    }),
  )
  return { view, props, events }
}

test('index changes preserve column order and stage edits without modifying metadata', async (t) => {
  const writes = []
  const { view, events } = setup(t, async (mutation) => {
    writes.push(mutation)
  })
  const original = { name: 'by_name', columns: ['name'], unique: false, editable: true }
  view.edit(original)
  view.draft.value.columns.unshift('id')
  assert.deepEqual(original.columns, ['name'])
  assert.equal(writes.length, 0)
  await view.save()
  assert.deepEqual(writes[0], {
    action: 'alterIndex',
    schema: 'public',
    table: 'items',
    expected: original,
    index: { ...original, columns: ['id', 'name'] },
  })
  assert.deepEqual(events, [['busy', true], ['saved'], ['busy', false]])
})

test('invalid, unchanged, and disabled index drafts cannot be applied', async (t) => {
  const writes = []
  const { view, props } = setup(t, async (mutation) => {
    writes.push(mutation)
  })
  view.edit()
  for (const columns of [[], ['id', 'id'], ['missing']]) {
    view.draft.value = { name: 'index', columns, unique: false, editable: true }
    await view.save()
  }
  const original = { name: 'index', columns: ['id'], unique: false, editable: true }
  view.edit(original)
  await view.save()
  props.disabled = true
  view.edit(original, true)
  await view.save()
  assert.equal(writes.length, 0)
})

test('cancelled deletion keeps the draft and never reports success', async (t) => {
  const { view, events } = setup(t, async (mutation) => {
    assert.equal(mutation.index, null)
    throw new DatabaseChangeCancelled()
  })
  const original = { name: 'index', columns: ['id'], unique: false, editable: true }
  view.edit(original, true)
  await view.save()
  assert.ok(view.draft.value)
  assert.equal(view.error.value, '')
  assert.deepEqual(events, [
    ['busy', true],
    ['busy', false],
  ])
})
