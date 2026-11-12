import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import ts from 'typescript'
import * as vue from 'vue'
import * as mongoQuery from '../app/utils/mongoQuery.ts'
import * as jsonLanguage from '@codemirror/lang-json'
import * as highlighting from '@lezer/highlight'
import { DatabaseChangeCancelled } from '../app/utils/databaseChange.ts'

function setup(t, component, initial, inlineTemplate = false) {
  const { descriptor } = parse(
    readFileSync(new URL(`../app/components/${component}.vue`, import.meta.url), 'utf8'),
  )
  const compiled = compileScript(descriptor, { id: 'mongo-component-test', inlineTemplate })
  const javascript = ts.transpileModule(compiled.content, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText
  const module = { exports: {} }
  new Function('require', 'module', 'exports', javascript)(
    (name) => {
      if (name === 'vue') return { ...vue, onMounted() {}, useTemplateRef: () => vue.ref(null) }
      if (name.endsWith('mongoQuery')) return mongoQuery
      if (name.endsWith('databaseChange')) return { DatabaseChangeCancelled }
      if (name === '@codemirror/lang-json') return jsonLanguage
      if (name === '@lezer/highlight') return highlighting
      return {}
    },
    module,
    module.exports,
  )
  const scope = vue.effectScope()
  t.after(() => scope.stop())
  const props = vue.reactive(initial)
  const events = []
  const view = scope.run(() =>
    module.exports.default.setup(props, { expose() {}, emit: (...event) => events.push(event) }),
  )
  return { view, props, events }
}

test('document actions menu edits and deletes without expanding and disables unavailable writes', (t) => {
  const {
    view: render,
    props,
    events,
  } = setup(
    t,
    'MongoDocumentCard',
    {
      row: { _id: 'one' },
      identity: 'one',
      editable: true,
      disabled: false,
      editDisabledReason: '',
    },
    true,
  )
  const button = render({}, []).children[0].children[2].children[0]
  assert.equal(button.props['aria-label'], 'Document actions')
  assert.equal(button.props['aria-haspopup'], 'menu')
  let stopped = false
  let prevented = false
  button.props.onClick({
    stopPropagation() {
      stopped = true
    },
    preventDefault() {
      prevented = true
    },
    currentTarget: { getBoundingClientRect: () => ({ left: 10, bottom: 20 }) },
  })
  assert.equal(stopped && prevented, true)
  const menu = () =>
    render({}, []).children.find((node) => node?.props?.label === 'Document actions')
  assert.equal(menu().props.x, 10)
  for (const [index, action] of ['edit', 'delete'].entries()) {
    assert.equal(menu().props.actions[index].disabled, false)
    menu().props.actions[index].run()
    assert.deepEqual(events[index], [action])
  }
  props.editable = false
  props.editDisabledReason = 'This connection is read only.'
  for (const action of menu().props.actions.slice(0, 2)) assert.equal(action.disabled, true)
  assert.equal(menu().props.actions[2].label, 'Copy document as JSON')
  assert.equal(menu().props.actions[2].disabled, false)
  menu().props.onClose()
  assert.equal(render({}, []).children[0].children[2].children[0].props['aria-expanded'], false)
})

test('document inspection keeps typed JSON intact when copying and reports clipboard failures', async (t) => {
  const row = {
    _id: { $oid: '507f1f77bcf86cd799439011' },
    n: { $numberLong: '9007199254740993' },
    text: '<b>literal</b>',
  }
  const original = Object.getOwnPropertyDescriptor(navigator, 'clipboard')
  t.after(() => {
    if (original) Object.defineProperty(navigator, 'clipboard', original)
    else delete navigator.clipboard
  })
  let copied
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: {
      writeText: async (text) => {
        copied = text
      },
    },
  })
  const { view } = setup(t, 'MongoDocumentCard', { row })
  assert.equal(view.tree.value, true)
  await view.copyDocument()
  assert.equal(copied, JSON.stringify(row, null, 2))
  assert.equal(view.copyFeedback.value, 'Document copied.')
  navigator.clipboard.writeText = async () => {
    throw new Error('denied')
  }
  await view.copyDocument()
  assert.match(view.copyFeedback.value, /Could not copy/)
})

test('document tree preserves nested values, array indexes and field order', (t) => {
  const row = {
    z: { $numberLong: '9007199254740993' },
    a: [null, { nested: true }],
    html: '<script>literal</script>',
  }
  const { view } = setup(t, 'JsonTree', { value: row, depth: 0 })
  assert.deepEqual(
    view.entries.value.map(([key]) => key),
    ['z', 'a', 'html'],
  )
  assert.equal(view.entries.value[0][1].$numberLong, '9007199254740993')
  assert.equal(view.isContainer(null), false)
  assert.equal(view.isContainer({}), false)
  assert.equal(view.isContainer(row.z), true)
  assert.equal(view.describe(row.a), 'Array (2)')
  assert.equal(view.describe(row.z), 'Object (1)')
  const array = setup(t, 'JsonTree', { value: row.a, depth: 1 })
  assert.deepEqual(array.view.entries.value, [
    ['0', null],
    ['1', { nested: true }],
  ])
})

test('MongoDB toolbar groups collection and query actions and protects writes', (t) => {
  const {
    view: render,
    props,
    events,
  } = setup(
    t,
    'MongoWorkspace',
    {
      table: { schema: 'catalog', name: 'items' },
      data: { rows: [] },
      activeQuery: null,
      readOnly: false,
      busy: false,
      page: 0,
      pageSize: 25,
    },
    true,
  )
  function menus() {
    const nodes = [render({}, [])]
    while (nodes.length) {
      const node = nodes.shift()
      if (node?.props?.menus) return node.props.menus
      if (Array.isArray(node?.children)) nodes.push(...node.children)
    }
    assert.fail('Missing toolbar menus')
  }
  assert.deepEqual(
    menus().map((menu) => menu.label),
    ['Collection', 'Query'],
  )
  const actions = () => menus().flatMap((menu) => menu.actions)
  actions()[1].run()
  actions()[2].run()
  assert.deepEqual(events, [['refresh'], ['export']])
  props.readOnly = true
  assert.equal(actions()[0].disabled, true)
  assert.equal(actions()[2].disabled, false)
  props.busy = true
  for (const action of actions()) assert.equal(action.disabled, true)
  props.busy = false
  props.table = null
  assert.equal(actions()[0].disabled, true)
  assert.equal(actions()[1].disabled, true)
})

test('MongoDB document drafts require confirmation and preserve typed values when saved', async (t) => {
  const writes = []
  const { view, events } = setup(t, 'MongoDocumentDialog', {
    mode: 'replace',
    table: { schema: 'catalog', name: 'items' },
    row: { _id: { $oid: '507f1f77bcf86cd799439011' }, n: { $numberLong: '9007199254740993' } },
    snapshot: 'original-bson',
    apply: async (mutation) => {
      writes.push(mutation)
      return { affectedRows: 1 }
    },
  })
  view.document.value = JSON.stringify({
    _id: { $oid: '507f1f77bcf86cd799439011' },
    n: { $numberLong: '9007199254740994' },
  })
  await vue.nextTick()
  await view.save()
  assert.equal(writes.length, 0)
  view.confirmation.value = 'APPLY'
  await view.save()
  assert.equal(writes[0].action, 'replaceDocument')
  assert.equal(writes[0].snapshot, 'original-bson')
  assert.equal(writes[0].document.n.$numberLong, '9007199254740994')
  assert.deepEqual(events, [['saved']])
  view.confirmation.value = 'APPLY'
  view.document.value = '{}'
  await vue.nextTick()
  assert.equal(view.confirmation.value, '')
})

test('MongoDB invalid drafts, stale writes and cancelled saves do not report success', async (t) => {
  let writes = 0
  const { view, props, events } = setup(t, 'MongoDocumentDialog', {
    mode: 'insert',
    table: { schema: 'catalog', name: 'items' },
    apply: async () => {
      writes++
      throw new Error('The document changed. Refresh.')
    },
  })
  view.document.value = '{"n":9007199254740993}'
  await vue.nextTick()
  view.confirmation.value = 'APPLY'
  await view.save()
  assert.equal(writes, 0)
  assert.match(view.error.value, /safe numbers/)
  view.document.value = '{}'
  await vue.nextTick()
  view.confirmation.value = 'APPLY'
  await view.save()
  assert.equal(writes, 1)
  assert.deepEqual(events, [])
  assert.match(view.error.value, /changed/)
  assert.equal(view.confirmation.value, '')
  props.apply = async () => {
    throw new DatabaseChangeCancelled()
  }
  view.confirmation.value = 'APPLY'
  await view.save()
  assert.deepEqual(events, [['close']])
})

test('MongoDB workspace blocks editing read-only or partial results and switches query modes', async (t) => {
  const row = { _id: 'one' }
  const { view, props, events } = setup(t, 'MongoWorkspace', {
    table: { schema: 'catalog', name: 'items' },
    data: { rows: [row] },
    activeQuery: null,
    readOnly: false,
    busy: false,
    page: 0,
    pageSize: 25,
  })
  view.edit('replace', row, 0)
  assert.equal(view.editor.value, null)
  assert.match(view.editDisabledReason(0), /Restart DBsaur/)
  props.data.editing = { originals: [{ snapshot: 'bson' }] }
  props.readOnly = true
  view.edit('replace', row, 0)
  assert.equal(view.editor.value, null)
  assert.match(view.editDisabledReason(0), /read only/)
  props.readOnly = false
  assert.equal(view.editDisabledReason(0), '')
  view.edit('delete', row, 0)
  assert.equal(view.editor.value.snapshot, 'bson')
  view.mode.value = 'aggregate'
  view.pipeline.value = '[{"$count":"total"}]'
  view.find()
  assert.deepEqual(JSON.parse(events[0][1]), {
    collection: 'items',
    pipeline: [{ $count: 'total' }],
  })
  props.activeQuery = events[0][1]
  await vue.nextTick()
  assert.equal(view.mode.value, 'aggregate')
  assert.match(view.editDisabledReason(0), /Aggregation/)
  view.editor.value = null
  view.edit('replace', row, 0)
  assert.equal(view.editor.value, null)
  view.reset()
  assert.deepEqual(JSON.parse(events[1][1]), {
    collection: 'items',
    filter: {},
    sort: {},
    projection: {},
  })
  props.activeQuery = '{"collection":"items","projection":{"name":1}}'
  await vue.nextTick()
  assert.match(view.editDisabledReason(0), /Clear Projection/)
  props.activeQuery = null
  props.data.editing.originals = [{}]
  await vue.nextTick()
  assert.match(view.editDisabledReason(0), /1 MiB/)
})

test('JSON view highlights keys, strings and BSON numbers while preserving literal text', (t) => {
  const value = {
    _id: { $numberLong: '9007199254740993' },
    html: '<script>alert(1)</script>',
    active: true,
    n: 42,
    empty: null,
  }
  const { view } = setup(t, 'JsonView', { value })
  const tokens = view.tokens.value
  assert.equal(tokens.map((token) => token.text).join(''), JSON.stringify(value, null, 2))
  for (const className of ['tok-propertyName', 'tok-string', 'tok-number', 'tok-bool', 'tok-null'])
    assert.ok(
      tokens.some((token) => token.className.includes(className)),
      className,
    )
})
