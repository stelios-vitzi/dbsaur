import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import ts from 'typescript'
import * as vue from 'vue'
import { layoutSchemaTables } from '../app/utils/schemaFlowLayout.ts'
import { DatabaseChangeCancelled } from '../app/utils/databaseChange.ts'
import * as diagramScope from '../app/utils/schemaDiagramScope.ts'

// Exercise the component's actual setup and pointer actions without a browser or database.
const source = readFileSync(new URL('../app/components/SchemaDiagram.vue', import.meta.url), 'utf8')
const { descriptor } = parse(source)
const compiled = compileScript(descriptor, { id: 'diagram-interaction' })
const javascript = ts.transpileModule(compiled.content, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText

function table(name, keys = []) {
  return {
    schema: 'public',
    name,
    kind: 'BASE TABLE',
    columns: ['id', 'parent_id', 'name'],
    primaryKeys: keys,
    referenceableColumns: keys.length === 1 ? [...keys] : [],
    keyConstraints: keys.length
      ? [{ name: `${name}_pkey`, kind: 'PRIMARY KEY', columns: keys }]
      : [],
    foreignKeys: [],
  }
}

function setup(t, { tables, selectedTable = null } = {}) {
  const parent = table('parents', ['id'])
  const child = table('children', ['id'])
  const writes = []
  const props = vue.reactive({
    database: 'sample',
    tables: tables ?? [parent, child],
    selectedTable,
    loading: false,
    editable: true,
    disabled: false,
    applyRelationship: async (link) => writes.push({ ...link }),
    updateRelationship: async () => {},
    removeRelationship: async () => {},
  })
  const module = { exports: {} }
  const runtime = {
    ref: vue.ref,
    shallowRef: vue.shallowRef,
    computed: vue.computed,
    watch: vue.watch,
    nextTick: vue.nextTick,
    onMounted: () => {},
    useTemplateRef: () => vue.ref(null),
    document: { elementFromPoint: () => null },
    window: { setTimeout: () => {} },
  }
  new Function('require', 'module', 'exports', ...Object.keys(runtime), javascript)(
    (name) =>
      name === 'vue'
        ? { ...vue, ...runtime }
        : name === '../utils/databaseChange'
          ? { DatabaseChangeCancelled }
          : name === '../utils/schemaDiagramScope'
            ? diagramScope
            : { layoutSchemaTables },
    module,
    module.exports,
    ...Object.values(runtime),
  )
  const scope = vue.effectScope()
  const diagram = scope.run(() => module.exports.default.setup(props, { expose() {}, emit() {} }))
  t.after(() => scope.stop())
  return { diagram, props, parent: props.tables[0], child: props.tables[1], writes }
}

function pointer(x, y, type = 'pointermove') {
  return {
    button: 0,
    pointerId: 1,
    clientX: x,
    clientY: y,
    type,
    preventDefault() {},
    currentTarget: { setPointerCapture() {} },
  }
}

test('nodes cross the old canvas origin and keep their positions after schema refresh', async (t) => {
  const { diagram, props, parent } = setup(t)
  diagram.startNodeDrag(pointer(100, 100), parent)
  diagram.movePointer(pointer(-5000, -5000))
  diagram.stopPointer(pointer(-5000, -5000, 'pointerup'))
  const position = { ...diagram.positionOf(parent) }
  assert.ok(position.x < 0 && position.y < 0)
  props.tables = props.tables.map((item) => ({ ...item }))
  await vue.nextTick()
  assert.deepEqual(diagram.positionOf(parent), position)
})

test('releasing a line on empty canvas keeps an editable source without an error or a write', (t) => {
  const { diagram, child, writes } = setup(t)
  diagram.startFieldLink(pointer(100, 100), child, 'parent_id', 'right')
  diagram.stopPointer(pointer(400, 300, 'pointerup'))
  assert.equal(diagram.pendingRelationship.value.foreignColumn, 'parent_id')
  assert.equal(diagram.pendingRelationship.value.referencedTable, '')
  assert.equal(diagram.relationshipError.value, '')
  assert.equal(diagram.relationshipReady.value, false)
  assert.equal(writes.length, 0)
})

test('a missing unique key is explained before apply and becomes ready after key preparation', async (t) => {
  const { diagram, parent, child, writes } = setup(t)
  diagram.beginRelationship(child, 'parent_id')
  diagram.chooseLinkTarget(parent, 'name')
  assert.equal(diagram.targetNeedsKey.value, true)
  assert.match(
    diagram.relationshipGuidance.value,
    /parents.name needs its own primary or unique key/,
  )
  await diagram.applyPendingRelationship()
  assert.equal(writes.length, 0)
  parent.keyConstraints.push({ name: 'parents_name_key', kind: 'UNIQUE', columns: ['name'] })
  parent.referenceableColumns.push('name')
  assert.equal(diagram.relationshipReady.value, true)
  await diagram.applyPendingRelationship()
  assert.equal(writes.length, 1)
  assert.equal(writes[0].referencedColumn, 'name')
})

test('dragging from a primary key to an ordinary field infers the foreign field and permits reversal', (t) => {
  const { diagram, parent, child, writes } = setup(t)
  diagram.beginRelationship(parent, 'id')
  diagram.chooseLinkTarget(child, 'parent_id')
  assert.equal(diagram.pendingRelationship.value.foreignTable, 'children')
  assert.equal(diagram.pendingRelationship.value.referencedTable, 'parents')
  assert.equal(diagram.relationshipReady.value, true)
  assert.match(diagram.relationshipNotice.value, /Using parents.id as the key/)
  diagram.reversePendingRelationship()
  assert.equal(diagram.pendingRelationship.value.foreignTable, 'parents')
  assert.equal(diagram.targetNeedsKey.value, true)
  assert.equal(writes.length, 0)
})

test('one field of a composite key is not offered as a complete reference key', (t) => {
  const { diagram, parent, child } = setup(t)
  parent.primaryKeys = ['id', 'name']
  parent.referenceableColumns = []
  parent.keyConstraints = [{ name: 'composite_pkey', kind: 'PRIMARY KEY', columns: ['id', 'name'] }]
  diagram.beginRelationship(child, 'parent_id')
  diagram.chooseLinkTarget(parent, 'id')
  assert.equal(diagram.targetNeedsKey.value, true)
  assert.equal(diagram.relationshipReady.value, false)
})

test('eligible standalone indexes are usable without a named unique constraint', (t) => {
  const { diagram, parent, child } = setup(t)
  parent.keyConstraints = []
  parent.primaryKeys = []
  parent.referenceableColumns = ['name']
  diagram.beginRelationship(child, 'parent_id')
  diagram.chooseLinkTarget(parent, 'name')
  assert.equal(diagram.referenceKind(parent, 'name'), 'indexed')
  assert.equal(diagram.relationshipReady.value, true)
})

test('choosing a hidden field reveals its actual row for the relationship endpoint', (t) => {
  const { diagram, parent, child } = setup(t)
  parent.columns = Array.from({ length: 12 }, (_, index) => `field_${index}`)
  parent.referenceableColumns = ['field_11']
  diagram.beginRelationship(child, 'parent_id')
  diagram.chooseLinkTarget(parent, 'field_11')
  assert.equal(diagram.displayedColumnCount(parent), 12)
  assert.equal(diagram.pendingRelationship.value.referencedColumn, 'field_11')
})

test('database rejection preserves the chosen fields and offers a specific recovery instruction', async (t) => {
  const { diagram, props, parent, child } = setup(t)
  props.applyRelationship = async () => {
    throw new Error('Foreign key field types are incompatible.')
  }
  diagram.beginRelationship(child, 'parent_id')
  diagram.chooseLinkTarget(parent, 'id')
  await diagram.applyPendingRelationship()
  assert.equal(diagram.pendingRelationship.value.foreignColumn, 'parent_id')
  assert.equal(diagram.pendingRelationship.value.referencedColumn, 'id')
  assert.match(diagram.relationshipFailureHint.value, /matching database types/)
  assert.equal(diagram.applyingRelationship.value, false)
})

// A chain hub <- t00 <- t01 ... plus many unrelated tables, larger than the full-view limit.
function largeSchema() {
  const tables = Array.from({ length: 40 }, (_, index) =>
    table(`t${String(index).padStart(2, '0')}`, ['id']),
  )
  const link = (from, to) =>
    tables[from].foreignKeys.push({
      name: `${tables[from].name}_${tables[to].name}_fkey`,
      column: 'parent_id',
      referencedSchema: 'public',
      referencedTable: tables[to].name,
      referencedColumn: 'id',
    })
  link(1, 0)
  link(2, 1)
  link(3, 2)
  link(4, 0)
  return tables
}

const names = (diagram) => diagram.visibleTables.value.map((item) => item.name)

test('small schemas open with every table in view', (t) => {
  const { diagram } = setup(t)
  assert.deepEqual(names(diagram), ['children', 'parents'])
  assert.equal(diagram.scopeAll.value, true)
})

test('large schemas open around the selected table instead of drawing everything', (t) => {
  const tables = largeSchema()
  const { diagram } = setup(t, { tables, selectedTable: tables[1] })
  assert.deepEqual(names(diagram), ['t00', 't01', 't02'])
  assert.equal(diagram.edges.value.length, 2)
  assert.equal(diagram.hiddenNeighborCount(tables[0]), 1)
  assert.equal(diagram.hiddenRelationshipCount.value, 2)
  diagram.changeFocusDepth(2)
  assert.deepEqual(names(diagram), ['t00', 't01', 't02', 't03', 't04'])
})

test('large schemas without a selection start empty with connected suggestions', (t) => {
  const { diagram } = setup(t, { tables: largeSchema() })
  assert.equal(diagram.visibleTables.value.length, 0)
  assert.deepEqual(
    diagram.startingTables.value.map((entry) => entry.table.name),
    ['t00', 't01', 't02', 't03', 't04'],
  )
})

test('expanding, hiding, and finding tables adjust only the scoped view', (t) => {
  const tables = largeSchema()
  const { diagram } = setup(t, { tables, selectedTable: tables[3] })
  assert.deepEqual(names(diagram), ['t02', 't03'])
  diagram.expandTable(tables[2])
  assert.deepEqual(names(diagram), ['t01', 't02', 't03'])
  diagram.hideTable(tables[3])
  assert.deepEqual(names(diagram), ['t01', 't02'])
  assert.equal(diagram.focusKey.value, null)
  diagram.finder.value = 't39'
  diagram.chooseFinderTable(diagram.finderMatches.value[0], 'add')
  assert.deepEqual(names(diagram), ['t01', 't02', 't39'])
  assert.equal(diagram.finder.value, '')
})

test('choosing a relationship target outside the view brings it into view', (t) => {
  const tables = largeSchema()
  const { diagram } = setup(t, { tables, selectedTable: tables[3] })
  diagram.beginRelationship(tables[3], 'parent_id')
  diagram.changeRelationshipTarget(diagram.tableKey(tables[30]))
  assert.ok(names(diagram).includes('t30'))
  assert.equal(diagram.pendingRelationship.value.referencedTable, 't30')
})

test('a table created while scoped is added to the view', async (t) => {
  const tables = largeSchema()
  const { diagram, props } = setup(t, { tables, selectedTable: tables[3] })
  props.tables = [...props.tables, table('new_table', ['id'])]
  await vue.nextTick()
  assert.ok(names(diagram).includes('new_table'))
})
