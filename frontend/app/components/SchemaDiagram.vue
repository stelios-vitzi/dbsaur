<script setup lang="ts">
import { ref, shallowRef, computed, watch, onMounted, nextTick, useTemplateRef } from 'vue'
import AppIcon from './AppIcon.vue'

import { DatabaseChangeCancelled } from '../utils/databaseChange'
import type { RelationshipChange, TableInfo } from '../../shared/database'
import { layoutSchemaTables } from '../utils/schemaFlowLayout'

const props = defineProps<{
  database: string
  tables: TableInfo[]
  selectedTable: TableInfo | null
  loading: boolean
  editable: boolean
  disabled: boolean
  applyRelationship: (relationship: RelationshipChange) => Promise<void>
  updateRelationship: (current: RelationshipChange, next: RelationshipChange) => Promise<void>
  removeRelationship: (relationship: RelationshipChange) => Promise<void>
}>()
const emit = defineEmits<{
  close: []
  choose: [table: TableInfo]
  createTable: []
  editTable: [table: TableInfo]
  prepareReference: [table: TableInfo, column: string]
}>()

const nodeWidth = 292
const columnHeight = 30
const shownColumns = 8
const connectionSides = ['left', 'right'] as const
const schemaColors = ['#93b8f2', '#8fc9a6', '#d9b77e', '#c2a6e8', '#e6a39d', '#8ac9d0']
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const viewport = useTemplateRef<HTMLDivElement>('viewport')
const filter = ref('')
const zoom = ref(1)
const pan = ref({ x: 0, y: 0 })
const positions = ref<Record<string, { x: number; y: number }>>({})
const expandedTables = ref(new Set<string>())
const hoveredTable = ref<string | null>(null)
const activeTable = ref<string | null>(null)
const moving = shallowRef<
  | { kind: 'pan'; pointerId: number; x: number; y: number; originX: number; originY: number }
  | {
      kind: 'node'
      pointerId: number
      key: string
      x: number
      y: number
      originX: number
      originY: number
    }
  | {
      kind: 'link'
      pointerId: number
      sourceKey: string
      sourceTable: TableInfo
      sourceColumn: string
      sourceSide: 'left' | 'right'
      x: number
      y: number
      originX: number
      originY: number
    }
  | null
>(null)
const pendingRelationship = ref<RelationshipChange | null>(null)
const selectedRelationship = ref<RelationshipChange | null>(null)
const relationshipDraft = ref<RelationshipChange | null>(null)
const relationshipTargetKey = ref('')
const applyingRelationship = ref(false)
const relationshipError = ref('')
const relationshipNotice = ref('')
const linkHover = shallowRef<{ table: TableInfo; column: string } | null>(null)
let ignoreFieldClick = false
const activeRelationship = computed(() => relationshipDraft.value ?? pendingRelationship.value)

function referenceKind(table: TableInfo, column: string) {
  if (table.referenceableColumns !== undefined) {
    if (!table.referenceableColumns.includes(column)) return 'needs-key'
    if (primaryKeys(table).length === 1 && primaryKeys(table)[0] === column) return 'primary'
    return table.keyConstraints?.some(
      (key) => key.kind === 'UNIQUE' && key.columns.length === 1 && key.columns[0] === column,
    )
      ? 'unique'
      : 'indexed'
  }
  if (primaryKeys(table).length === 1 && primaryKeys(table)[0] === column) return 'primary'
  if (
    table.keyConstraints?.some(
      (key) => key.kind === 'UNIQUE' && key.columns.length === 1 && key.columns[0] === column,
    )
  )
    return 'unique'
  return table.keyConstraints === undefined ? 'unknown' : 'needs-key'
}
function referenceLabel(table: TableInfo, column: string) {
  const kind = referenceKind(table, column)
  return kind === 'primary'
    ? 'Primary key'
    : kind === 'unique'
      ? 'Unique key'
      : kind === 'indexed'
        ? 'Reference index'
        : kind === 'unknown'
          ? 'Key details unavailable'
          : 'Needs a unique key'
}
const targetNeedsKey = computed(
  () =>
    !!relationshipTarget.value &&
    !!activeRelationship.value?.referencedColumn &&
    referenceKind(relationshipTarget.value, activeRelationship.value.referencedColumn) ===
      'needs-key',
)
const relationshipReady = computed(() => {
  const link = activeRelationship.value
  return (
    !!link &&
    !!relationshipSource.value?.columns.includes(link.foreignColumn) &&
    !!relationshipTarget.value?.columns.includes(link.referencedColumn) &&
    !(
      link.foreignSchema === link.referencedSchema &&
      link.foreignTable === link.referencedTable &&
      link.foreignColumn === link.referencedColumn
    ) &&
    !targetNeedsKey.value
  )
})
const relationshipGuidance = computed(() => {
  const link = activeRelationship.value
  if (!link?.referencedTable || !link.referencedColumn)
    return 'Choose the table and field whose values this field should reference. Highlighted key fields are ready to link.'
  if (targetNeedsKey.value)
    return `${link.referencedTable}.${link.referencedColumn} needs its own primary or unique key so each value identifies one row. Choose a key field, or edit this table’s keys below.`
  if (!relationshipReady.value)
    return 'Choose two different fields. A field cannot reference itself.'
  return `Each value in ${link.foreignTable}.${link.foreignColumn} will need a matching ${link.referencedTable}.${link.referencedColumn}. Field types must match; existing rows must also follow this rule.`
})
const relationshipFailureHint = computed(() => {
  const error = relationshipError.value
  if (/types?.*incompatible|type.*mismatch/i.test(error))
    return 'Choose fields with matching database types in the selectors above. For example, an integer reference should point to an integer key.'
  if (/existing rows|no matching|foreign key violation|constraint violation/i.test(error))
    return 'Some existing values may have no matching row in the referenced table. Review those values before retrying, or choose a different pair of fields.'
  if (/permission|privilege/i.test(error))
    return 'This connection needs permission to alter the table and create its foreign key. Ask the database administrator for access, then retry.'
  return 'Your field choices are preserved. Review the reason above, adjust the selected fields, and retry.'
})

function tableKey(table: Pick<TableInfo, 'schema' | 'name'>) {
  return JSON.stringify([table.schema, table.name])
}
function foreignKeys(table: TableInfo) {
  return table.foreignKeys ?? []
}
function primaryKeys(table: TableInfo) {
  return table.primaryKeys ?? []
}
function isForeignKey(table: TableInfo, column: string) {
  return foreignKeys(table).some((key) => key.column === column)
}
function isRelated(table: TableInfo) {
  const key = hoveredTable.value ?? activeTable.value
  if (!key) return false
  return edges.value.some(
    (edge) =>
      (edge.sourceKey === key && edge.targetKey === tableKey(table)) ||
      (edge.targetKey === key && edge.sourceKey === tableKey(table)),
  )
}
function isSelectedRelationship(relationship: RelationshipChange) {
  const selected = selectedRelationship.value
  return (
    !!selected &&
    selected.foreignSchema === relationship.foreignSchema &&
    selected.foreignTable === relationship.foreignTable &&
    selected.constraintName === relationship.constraintName
  )
}
function relationshipFieldCount(table: TableInfo) {
  let lastLinkedIndex = -1
  for (const key of foreignKeys(table))
    lastLinkedIndex = Math.max(lastLinkedIndex, table.columns.indexOf(key.column))
  for (const candidate of props.tables) {
    for (const key of foreignKeys(candidate)) {
      if (key.referencedSchema === table.schema && key.referencedTable === table.name)
        lastLinkedIndex = Math.max(lastLinkedIndex, table.columns.indexOf(key.referencedColumn))
    }
  }
  return Math.max(shownColumns, lastLinkedIndex + 1)
}
function displayedColumnCount(table: TableInfo) {
  return expandedTables.value.has(tableKey(table))
    ? table.columns.length
    : Math.min(table.columns.length, relationshipFieldCount(table))
}
function toggleTableFields(table: TableInfo) {
  const next = new Set(expandedTables.value)
  const key = tableKey(table)
  if (next.has(key)) next.delete(key)
  else next.add(key)
  expandedTables.value = next
}
function tableHeight(table: TableInfo) {
  const visible = displayedColumnCount(table)
  const hasMore = visible < table.columns.length
  const fieldsHeight = table.columns.length ? 10 + visible * columnHeight + (hasMore ? 30 : 0) : 41
  return 56 + fieldsHeight + 39
}
const sortedTables = computed(() =>
  [...props.tables].sort(
    (a, b) => a.schema.localeCompare(b.schema) || a.name.localeCompare(b.name),
  ),
)
const flowPositions = computed(() => layoutSchemaTables(sortedTables.value))
watch(
  flowPositions,
  (layout, previousLayout) => {
    positions.value = Object.fromEntries(
      Object.entries(layout).map(([key, point]) => [key, positions.value[key] ?? point]),
    )
    if (previousLayout && Object.keys(previousLayout).length === 0) void fitView()
  },
  { immediate: true },
)
function positionOf(table: TableInfo) {
  return positions.value[tableKey(table)] ?? { x: 72, y: 72 }
}
const visibleTables = computed(() => {
  const query = filter.value.trim().toLowerCase()
  if (!query) return sortedTables.value
  return sortedTables.value.filter((table) =>
    `${table.schema}.${table.name} ${table.columns.join(' ')}`.toLowerCase().includes(query),
  )
})
const relationshipMetadataMissing = computed(
  () =>
    props.tables.length > 0 && props.tables.every((table) => !Object.hasOwn(table, 'foreignKeys')),
)
const relationshipTables = computed(() =>
  props.tables.filter((table) => table.kind === 'BASE TABLE'),
)
const relationshipSource = computed(() =>
  activeRelationship.value
    ? props.tables.find(
        (table) =>
          table.schema === activeRelationship.value!.foreignSchema &&
          table.name === activeRelationship.value!.foreignTable,
      )
    : undefined,
)
const relationshipTarget = computed(() =>
  activeRelationship.value
    ? props.tables.find(
        (table) =>
          table.schema === activeRelationship.value!.referencedSchema &&
          table.name === activeRelationship.value!.referencedTable,
      )
    : undefined,
)
watch(
  () => [
    activeRelationship.value?.foreignSchema,
    activeRelationship.value?.foreignTable,
    activeRelationship.value?.foreignColumn,
    activeRelationship.value?.referencedSchema,
    activeRelationship.value?.referencedTable,
    activeRelationship.value?.referencedColumn,
  ],
  () => {
    const link = activeRelationship.value
    if (!link) return
    for (const [table, column] of [
      [relationshipSource.value, link.foreignColumn],
      [relationshipTarget.value, link.referencedColumn],
    ] as const) {
      if (table && table.columns.indexOf(column) >= displayedColumnCount(table)) {
        expandedTables.value = new Set([...expandedTables.value, tableKey(table)])
      }
    }
  },
  { flush: 'sync' },
)
const canApplyRelationshipDraft = computed(
  () =>
    !!relationshipDraft.value?.constraintName &&
    relationshipReady.value &&
    !applyingRelationship.value &&
    !props.disabled,
)
const visibleKeys = computed(() => new Set(visibleTables.value.map(tableKey)))
const relationships = computed(() =>
  visibleTables.value.flatMap((source) =>
    foreignKeys(source)
      .filter((key) =>
        visibleKeys.value.has(
          tableKey({ schema: key.referencedSchema, name: key.referencedTable }),
        ),
      )
      .map((foreignKey) => ({ source, foreignKey })),
  ),
)
const relationshipCount = computed(() =>
  props.tables.reduce(
    (count, table) =>
      count +
      foreignKeys(table).filter((key) =>
        props.tables.some(
          (target) => target.schema === key.referencedSchema && target.name === key.referencedTable,
        ),
      ).length,
    0,
  ),
)
function rowY(table: TableInfo, column: string) {
  const index = table.columns.indexOf(column)
  const visible = displayedColumnCount(table)
  if (index >= visible) return positionOf(table).y + 56 + 5 + visible * columnHeight + 15
  const row = index < 0 ? 0 : index
  return positionOf(table).y + 56 + 5 + row * columnHeight + columnHeight / 2
}
function fieldPoint(table: TableInfo, column: string, otherX: number) {
  const point = positionOf(table)
  const center = point.x + nodeWidth / 2
  const side = otherX < center ? 'left' : 'right'
  return fieldPointAtSide(table, column, side, 'output')
}
function fieldInputPoint(table: TableInfo, column: string, otherX: number) {
  const point = positionOf(table)
  const center = point.x + nodeWidth / 2
  const side = otherX < center ? 'left' : 'right'
  return fieldPointAtSide(table, column, side, 'input')
}
function fieldPointAtSide(
  table: TableInfo,
  column: string,
  side: 'left' | 'right',
  kind: 'input' | 'output',
) {
  const point = positionOf(table)
  const offset = kind === 'input' ? 7 : 17
  return {
    x: side === 'left' ? point.x + offset : point.x + nodeWidth - offset,
    y: rowY(table, column),
  }
}
function curvePath(start: { x: number; y: number }, end: { x: number; y: number }) {
  const direction = end.x >= start.x ? 1 : -1
  const bend = Math.max(52, Math.abs(end.x - start.x) * 0.4)
  return `M ${start.x} ${start.y} C ${start.x + bend * direction} ${start.y}, ${end.x - bend * direction} ${end.y}, ${end.x} ${end.y}`
}
function relationshipPath(relationship: RelationshipChange) {
  const foreignTable = props.tables.find(
    (table) =>
      table.schema === relationship.foreignSchema && table.name === relationship.foreignTable,
  )
  const referencedTable = props.tables.find(
    (table) =>
      table.schema === relationship.referencedSchema && table.name === relationship.referencedTable,
  )
  if (!foreignTable || !referencedTable) return ''
  const foreignCenter = positionOf(foreignTable).x + nodeWidth / 2
  const referencedCenter = positionOf(referencedTable).x + nodeWidth / 2
  if (foreignTable === referencedTable) {
    const foreignPoint = fieldPointAtSide(
      foreignTable,
      relationship.foreignColumn,
      'right',
      'output',
    )
    const referencedPoint = fieldPointAtSide(
      referencedTable,
      relationship.referencedColumn,
      'right',
      'input',
    )
    const foreignY = rowY(foreignTable, relationship.foreignColumn)
    const referencedY = rowY(referencedTable, relationship.referencedColumn)
    return `M ${foreignPoint.x} ${foreignY} C ${foreignPoint.x + 86} ${foreignY + 42}, ${referencedPoint.x + 86} ${referencedY - 42}, ${referencedPoint.x} ${referencedY}`
  }
  const start = fieldPoint(foreignTable, relationship.foreignColumn, referencedCenter)
  const end = fieldInputPoint(referencedTable, relationship.referencedColumn, foreignCenter)
  return curvePath(start, end)
}
function linkDragPath(drag: Extract<(typeof moving)['value'], { kind: 'link' }>) {
  const bounds = viewport.value?.getBoundingClientRect()
  if (!bounds) return ''
  const pointer = {
    x: (drag.x - bounds.left - pan.value.x) / zoom.value,
    y: (drag.y - bounds.top - pan.value.y) / zoom.value,
  }
  const start = fieldPointAtSide(drag.sourceTable, drag.sourceColumn, drag.sourceSide, 'output')
  const hover = linkHover.value
  const end =
    hover?.table.kind === 'BASE TABLE' &&
    !(tableKey(hover.table) === drag.sourceKey && hover.column === drag.sourceColumn)
      ? fieldInputPoint(hover.table, hover.column, positionOf(drag.sourceTable).x + nodeWidth / 2)
      : pointer
  return curvePath(start, end)
}
const edges = computed(() =>
  relationships.value.flatMap(({ source: foreignTable, foreignKey }) => {
    const referencedTable = props.tables.find(
      (table) =>
        table.schema === foreignKey.referencedSchema && table.name === foreignKey.referencedTable,
    )
    if (!referencedTable) return []
    const sourceKey = tableKey(foreignTable)
    const targetKey = tableKey(referencedTable)
    const relationship: RelationshipChange = {
      constraintName: foreignKey.name,
      foreignSchema: foreignTable.schema,
      foreignTable: foreignTable.name,
      foreignColumn: foreignKey.column,
      referencedSchema: foreignKey.referencedSchema,
      referencedTable: foreignKey.referencedTable,
      referencedColumn: foreignKey.referencedColumn,
    }
    return [
      {
        id: `${sourceKey}:${foreignKey.name}:${foreignKey.column}`,
        sourceKey,
        targetKey,
        relationship,
        label: `${foreignTable.schema}.${foreignTable.name}.${foreignKey.column} \u2192 ${referencedTable.schema}.${referencedTable.name}.${foreignKey.referencedColumn}`,
        path: relationshipPath(relationship),
      },
    ]
  }),
)
const planeSize = computed(() => {
  const nodes = visibleTables.value
  if (!nodes.length) return { width: 1200, height: 800 }
  return {
    width: Math.max(1200, ...nodes.map((table) => positionOf(table).x + nodeWidth + 100)),
    height: Math.max(800, ...nodes.map((table) => positionOf(table).y + tableHeight(table) + 100)),
  }
})
function schemaColor(schema: string) {
  const schemas = [...new Set(sortedTables.value.map((table) => table.schema))]
  return schemaColors[schemas.indexOf(schema) % schemaColors.length]!
}
function focusTable(table: TableInfo) {
  activeTable.value = tableKey(table)
}
function openTable(table: TableInfo) {
  emit('choose', table)
  dialog.value?.close()
}
function startPan(event: PointerEvent) {
  if (event.button !== 0 || event.target !== event.currentTarget) return
  const element = event.currentTarget as HTMLElement
  element.setPointerCapture(event.pointerId)
  moving.value = {
    kind: 'pan',
    pointerId: event.pointerId,
    x: event.clientX,
    y: event.clientY,
    originX: pan.value.x,
    originY: pan.value.y,
  }
}
function startNodeDrag(event: PointerEvent, table: TableInfo) {
  if (event.button !== 0) return
  const key = tableKey(table)
  const point = positionOf(table)
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  moving.value = {
    kind: 'node',
    pointerId: event.pointerId,
    key,
    x: event.clientX,
    y: event.clientY,
    originX: point.x,
    originY: point.y,
  }
}
function startFieldLink(
  event: PointerEvent,
  table: TableInfo,
  column: string,
  side: 'left' | 'right',
) {
  if (
    event.button !== 0 ||
    !props.editable ||
    props.disabled ||
    applyingRelationship.value ||
    pendingRelationship.value ||
    table.kind !== 'BASE TABLE'
  )
    return
  event.preventDefault()
  selectedRelationship.value = null
  relationshipDraft.value = null
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  relationshipError.value = ''
  moving.value = {
    kind: 'link',
    pointerId: event.pointerId,
    sourceKey: tableKey(table),
    sourceTable: table,
    sourceColumn: column,
    sourceSide: side,
    x: event.clientX,
    y: event.clientY,
    originX: event.clientX,
    originY: event.clientY,
  }
}
function connectionSide(event: PointerEvent): 'left' | 'right' {
  const row = event.currentTarget as HTMLElement
  return event.clientX < row.getBoundingClientRect().left + row.getBoundingClientRect().width / 2
    ? 'left'
    : 'right'
}
function fieldAtPoint(x: number, y: number, sourceX: number) {
  const element = document.elementFromPoint(x, y)
  const handle = element?.closest<HTMLElement>('[data-diagram-handle]')
  const row = element?.closest<HTMLElement>('[data-diagram-column-name]')
  const card = element?.closest<HTMLElement>('[data-diagram-node-key]')
  const key = row?.dataset.diagramTableKey ?? card?.dataset.diagramNodeKey
  const table = key && props.tables.find((item) => tableKey(item) === key)
  const column =
    row?.dataset.diagramColumnName ??
    (table
      ? (table.columns.find((column) =>
          ['primary', 'unique', 'indexed'].includes(referenceKind(table, column)),
        ) ?? table.columns[0])
      : undefined)
  const handleSide = handle?.dataset.diagramHandleSide
  const bounds = row?.getBoundingClientRect()
  const side =
    handleSide === 'left' || handleSide === 'right'
      ? handleSide
      : bounds && sourceX < bounds.left + bounds.width / 2
        ? 'left'
        : 'right'
  return table && column && (side === 'left' || side === 'right') ? { table, column, side } : null
}
function movePointer(event: PointerEvent) {
  const drag = moving.value
  if (!drag || drag.pointerId !== event.pointerId) return
  if (drag.kind === 'pan') {
    pan.value = {
      x: drag.originX + event.clientX - drag.x,
      y: drag.originY + event.clientY - drag.y,
    }
  } else if (drag.kind === 'node') {
    positions.value[drag.key] = {
      x: drag.originX + (event.clientX - drag.x) / zoom.value,
      y: drag.originY + (event.clientY - drag.y) / zoom.value,
    }
  } else {
    moving.value = { ...drag, x: event.clientX, y: event.clientY }
    linkHover.value = fieldAtPoint(event.clientX, event.clientY, drag.originX)
  }
}
function stopPointer(event: PointerEvent) {
  const drag = moving.value
  if (!drag || drag.pointerId !== event.pointerId) return
  if (drag.kind === 'link' && event.type === 'pointerup') {
    const moved = Math.hypot(event.clientX - drag.originX, event.clientY - drag.originY) >= 8
    ignoreFieldClick = moved
    if (moved)
      window.setTimeout(() => {
        ignoreFieldClick = false
      }, 0)
    beginRelationship(drag.sourceTable, drag.sourceColumn)
    if (moved) {
      const target = fieldAtPoint(event.clientX, event.clientY, drag.originX)
      if (target?.table.kind === 'BASE TABLE') chooseLinkTarget(target.table, target.column)
    }
  }
  moving.value = null
  linkHover.value = null
}
function beginRelationship(table?: TableInfo, column?: string) {
  const source =
    table ??
    relationshipTables.value.find(
      (item) => tableKey(item) === tableKey(props.selectedTable ?? { schema: '', name: '' }),
    ) ??
    relationshipTables.value[0]
  if (!source) return
  selectedRelationship.value = null
  relationshipDraft.value = null
  relationshipError.value = ''
  relationshipNotice.value = ''
  relationshipTargetKey.value = ''
  pendingRelationship.value = {
    foreignSchema: source.schema,
    foreignTable: source.name,
    foreignColumn: column ?? source.columns[0] ?? '',
    referencedSchema: '',
    referencedTable: '',
    referencedColumn: '',
  }
}
function chooseLinkTarget(table: TableInfo, column: string) {
  const link = pendingRelationship.value
  if (
    !link ||
    table.kind !== 'BASE TABLE' ||
    (link.foreignSchema === table.schema &&
      link.foreignTable === table.name &&
      link.foreignColumn === column)
  )
    return
  const source = relationshipSource.value
  if (
    source &&
    ['primary', 'unique', 'indexed'].includes(referenceKind(source, link.foreignColumn)) &&
    referenceKind(table, column) === 'needs-key'
  ) {
    pendingRelationship.value = {
      foreignSchema: table.schema,
      foreignTable: table.name,
      foreignColumn: column,
      referencedSchema: source.schema,
      referencedTable: source.name,
      referencedColumn: link.foreignColumn,
    }
    relationshipTargetKey.value = tableKey(source)
    relationshipNotice.value = `Using ${source.name}.${link.foreignColumn} as the key: ${table.name}.${column} will store the reference. You can reverse this direction below.`
    relationshipError.value = ''
    return
  }
  relationshipNotice.value = ''
  link.referencedSchema = table.schema
  link.referencedTable = table.name
  link.referencedColumn = column
  relationshipTargetKey.value = tableKey(table)
  relationshipError.value = ''
}
function clickField(table: TableInfo, column: string) {
  if (ignoreFieldClick) {
    ignoreFieldClick = false
    return
  }
  if (
    !props.editable ||
    props.disabled ||
    applyingRelationship.value ||
    table.kind !== 'BASE TABLE'
  )
    return
  if (pendingRelationship.value) chooseLinkTarget(table, column)
  else beginRelationship(table, column)
}
function changeRelationshipSource(key: string) {
  const table = relationshipTables.value.find((item) => tableKey(item) === key)
  if (table) beginRelationship(table)
}
function reversePendingRelationship() {
  const link = pendingRelationship.value
  if (!link?.referencedTable) return
  pendingRelationship.value = {
    foreignSchema: link.referencedSchema,
    foreignTable: link.referencedTable,
    foreignColumn: link.referencedColumn,
    referencedSchema: link.foreignSchema,
    referencedTable: link.foreignTable,
    referencedColumn: link.foreignColumn,
  }
  relationshipTargetKey.value = tableKey({ schema: link.foreignSchema, name: link.foreignTable })
  relationshipError.value = ''
  relationshipNotice.value = ''
}
function cancelPendingRelationship() {
  pendingRelationship.value = null
  selectedRelationship.value = null
  relationshipDraft.value = null
  relationshipError.value = ''
  relationshipNotice.value = ''
}
function selectRelationship(relationship: RelationshipChange) {
  if (!props.editable || props.disabled) return
  pendingRelationship.value = null
  selectedRelationship.value = relationship
  relationshipDraft.value = { ...relationship }
  relationshipTargetKey.value = tableKey({
    schema: relationship.referencedSchema,
    name: relationship.referencedTable,
  })
  relationshipError.value = ''
  relationshipNotice.value = ''
}
function changeRelationshipTarget(key: string) {
  relationshipTargetKey.value = key
  const targetTable = props.tables.find((table) => tableKey(table) === key)
  const link = activeRelationship.value
  if (!targetTable || !link) return
  link.referencedSchema = targetTable.schema
  link.referencedTable = targetTable.name
  if (!targetTable.columns.includes(link.referencedColumn))
    link.referencedColumn =
      targetTable.columns.find((column) =>
        ['primary', 'unique', 'indexed'].includes(referenceKind(targetTable, column)),
      ) ??
      targetTable.columns[0] ??
      ''
  relationshipError.value = ''
  relationshipNotice.value = ''
}
function clearRelationshipFeedback() {
  relationshipError.value = ''
  relationshipNotice.value = ''
}
async function applyPendingRelationship() {
  const relationship = pendingRelationship.value
  if (!relationship || !relationshipReady.value || applyingRelationship.value || props.disabled)
    return
  applyingRelationship.value = true
  relationshipError.value = ''
  try {
    await props.applyRelationship(relationship)
    pendingRelationship.value = null
  } catch (cause) {
    if (cause instanceof DatabaseChangeCancelled) {
      cancelPendingRelationship()
      return
    }
    relationshipError.value =
      cause instanceof Error ? cause.message : 'Could not apply this relationship.'
  } finally {
    applyingRelationship.value = false
  }
}
async function applyRelationshipEdit() {
  const current = selectedRelationship.value
  const next = relationshipDraft.value
  if (!current || !next || !canApplyRelationshipDraft.value || applyingRelationship.value) return
  applyingRelationship.value = true
  relationshipError.value = ''
  try {
    await props.updateRelationship(current, { ...next })
    cancelPendingRelationship()
  } catch (cause) {
    if (cause instanceof DatabaseChangeCancelled) {
      cancelPendingRelationship()
      return
    }
    relationshipError.value =
      cause instanceof Error ? cause.message : 'Could not update this relationship.'
  } finally {
    applyingRelationship.value = false
  }
}
async function removeSelectedRelationship() {
  const relationship = selectedRelationship.value
  if (!relationship || applyingRelationship.value || props.disabled) return
  applyingRelationship.value = true
  relationshipError.value = ''
  try {
    await props.removeRelationship(relationship)
    cancelPendingRelationship()
  } catch (cause) {
    if (cause instanceof DatabaseChangeCancelled) {
      cancelPendingRelationship()
      return
    }
    relationshipError.value =
      cause instanceof Error ? cause.message : 'Could not remove this relationship.'
  } finally {
    applyingRelationship.value = false
  }
}
function changeZoom(amount: number, focusX?: number, focusY?: number) {
  const element = viewport.value
  const pointX = focusX ?? (element?.clientWidth ?? 0) / 2
  const pointY = focusY ?? (element?.clientHeight ?? 0) / 2
  const next = Math.max(0.35, Math.min(1.6, Math.round((zoom.value + amount) * 100) / 100))
  if (next === zoom.value) return
  pan.value = {
    x: pointX - ((pointX - pan.value.x) / zoom.value) * next,
    y: pointY - ((pointY - pan.value.y) / zoom.value) * next,
  }
  zoom.value = next
}
function arrangeFlow() {
  positions.value = { ...flowPositions.value }
  void fitView()
}
function zoomAt(event: WheelEvent) {
  if (!viewport.value) return
  const bounds = viewport.value.getBoundingClientRect()
  changeZoom(
    event.deltaY < 0 ? 0.08 : -0.08,
    event.clientX - bounds.left,
    event.clientY - bounds.top,
  )
}
async function fitView() {
  await nextTick()
  const element = viewport.value
  if (!element) return
  const nodes = visibleTables.value
  if (!nodes.length) {
    zoom.value = 1
    pan.value = { x: 0, y: 0 }
    return
  }
  const minX = Math.min(...nodes.map((table) => positionOf(table).x))
  const minY = Math.min(...nodes.map((table) => positionOf(table).y))
  const maxX = Math.max(...nodes.map((table) => positionOf(table).x + nodeWidth))
  const maxY = Math.max(...nodes.map((table) => positionOf(table).y + tableHeight(table)))
  const boundsWidth = Math.max(1, maxX - minX)
  const boundsHeight = Math.max(1, maxY - minY)
  const scale = Math.max(
    0.35,
    Math.min(
      1.2,
      (element.clientWidth - 100) / boundsWidth,
      (element.clientHeight - 100) / boundsHeight,
    ),
  )
  zoom.value = scale
  pan.value = {
    x: (element.clientWidth - boundsWidth * scale) / 2 - minX * scale,
    y: (element.clientHeight - boundsHeight * scale) / 2 - minY * scale,
  }
}
function closeDialog() {
  dialog.value?.close()
}
function closeFromBackdrop(event: MouseEvent) {
  if (event.target === dialog.value) closeDialog()
}
onMounted(async () => {
  dialog.value?.showModal()
  await fitView()
})
watch(filter, fitView)
watch(
  () => props.tables.length,
  () => fitView(),
)
</script>

<template>
  <dialog
    ref="dialog"
    class="schema-diagram-modal"
    aria-label="Database schema diagram"
    @close="emit('close')"
    @click="closeFromBackdrop"
  >
    <div class="schema-diagram-shell">
      <header class="schema-diagram-header">
        <div class="schema-diagram-title">
          <span class="schema-diagram-mark"><AppIcon name="diagram" :size="21" /></span>
          <div>
            <h2>Database diagram</h2>
            <p>
              <strong>{{ database }}</strong>
              <span>{{ tables.length }} {{ tables.length === 1 ? 'table' : 'tables' }}</span>
              <span
                >{{ relationshipCount }}
                {{ relationshipCount === 1 ? 'relationship' : 'relationships' }}</span
              >
            </p>
          </div>
        </div>
        <div class="schema-diagram-actions">
          <button
            v-if="editable"
            type="button"
            class="button secondary small schema-diagram-create"
            aria-label="Link fields"
            title="Connect a field to a key in another table"
            :disabled="disabled || applyingRelationship || !relationshipTables.length"
            @click="beginRelationship()"
          >
            <AppIcon name="link" :size="16" /><span>Link fields</span>
          </button>
          <button
            v-if="editable"
            type="button"
            class="button secondary small schema-diagram-create"
            title="Add a table to this database"
            aria-label="Add table"
            :disabled="disabled"
            @click="emit('createTable')"
          >
            <AppIcon name="plus" :size="16" /><span>Add table</span>
          </button>
          <label class="search-box schema-diagram-search">
            <AppIcon name="search" :size="15" />
            <input
              v-model="filter"
              placeholder="Find tables or columns"
              aria-label="Find tables or columns"
            />
            <button
              v-if="filter"
              type="button"
              class="icon-button"
              aria-label="Clear search"
              @click="filter = ''"
            >
              <AppIcon name="close" :size="13" />
            </button>
          </label>
          <div class="diagram-zoom-controls" role="group" aria-label="Diagram zoom controls">
            <button
              type="button"
              class="icon-button"
              title="Zoom out"
              aria-label="Zoom out"
              @click="changeZoom(-0.1)"
            >
              <AppIcon name="zoomOut" :size="18" />
            </button>
            <span class="diagram-zoom-value" aria-live="polite">{{ Math.round(zoom * 100) }}%</span>
            <button
              type="button"
              class="icon-button"
              title="Zoom in"
              aria-label="Zoom in"
              @click="changeZoom(0.1)"
            >
              <AppIcon name="zoomIn" :size="18" />
            </button>
            <button
              type="button"
              class="button secondary small diagram-fit-button"
              title="Arrange tables by their relationships"
              @click="arrangeFlow"
            >
              <AppIcon name="diagram" :size="15" /><span>Arrange</span>
            </button>
            <button
              type="button"
              class="button secondary small diagram-fit-button"
              title="Fit tables to the canvas"
              @click="fitView"
            >
              <AppIcon name="fit" :size="16" /><span>Fit</span>
            </button>
          </div>
          <button
            type="button"
            class="icon-button diagram-close"
            title="Close diagram"
            aria-label="Close diagram"
            @click="closeDialog"
          >
            <AppIcon name="close" :size="18" />
          </button>
        </div>
      </header>

      <div class="schema-diagram-legend" aria-label="Diagram legend">
        <span><AppIcon name="key" :size="13" /> Primary key</span>
        <span><AppIcon name="check" :size="13" /> Unique key / reference index</span>
        <span><AppIcon name="link" :size="13" /> Foreign key</span>
        <span class="diagram-edge-key"><i /> Declared relationship</span>
        <span
          v-if="relationshipMetadataMissing"
          class="diagram-legend-hint relationship-metadata-warning"
        >
          Relationship metadata is unavailable. Restart DBsaur, reconnect, then reopen the diagram.
        </span>
        <span v-else class="diagram-legend-hint">
          {{
            editable
              ? 'Drag between fields, or click a field then its target. Key fields can receive references. Drag table headings to move; drag empty space to pan.'
              : 'Flow from source tables top to bottom. Drag cards to adjust. Drag empty space to pan.'
          }}
        </span>
      </div>

      <section
        v-if="
          moving?.kind !== 'link' &&
          (pendingRelationship || selectedRelationship || relationshipError)
        "
        class="diagram-relationship-editor"
        aria-live="polite"
      >
        <div class="diagram-relationship-copy">
          <strong>
            {{ selectedRelationship ? 'Edit relationship' : 'New relationship' }}
          </strong>
          <span v-if="activeRelationship">{{ relationshipGuidance }}</span>
          <span v-if="relationshipNotice" class="diagram-relationship-notice">{{
            relationshipNotice
          }}</span>
          <span v-if="relationshipError" class="diagram-relationship-error" role="alert">
            {{ relationshipError }}
          </span>
        </div>
        <div v-if="activeRelationship" class="diagram-relationship-fields">
          <label v-if="pendingRelationship">
            Table storing the value
            <select
              :value="
                tableKey({
                  schema: activeRelationship.foreignSchema,
                  name: activeRelationship.foreignTable,
                })
              "
              :disabled="disabled || applyingRelationship"
              @change="changeRelationshipSource(($event.target as HTMLSelectElement).value)"
            >
              <option
                v-for="table in relationshipTables"
                :key="tableKey(table)"
                :value="tableKey(table)"
              >
                {{ table.schema }}.{{ table.name }}
              </option>
            </select>
          </label>
          <label>
            Field storing the value
            <select
              v-model="activeRelationship.foreignColumn"
              @change="clearRelationshipFeedback"
              :disabled="disabled || applyingRelationship"
            >
              <option
                v-for="field in relationshipSource?.columns || []"
                :key="field"
                :value="field"
              >
                {{ field }}
              </option>
            </select>
          </label>
          <label>
            Table to reference
            <select
              :value="relationshipTargetKey"
              :disabled="disabled || applyingRelationship"
              @change="changeRelationshipTarget(($event.target as HTMLSelectElement).value)"
            >
              <option value="" disabled>Choose a table</option>
              <option
                v-for="table in relationshipTables"
                :key="tableKey(table)"
                :value="tableKey(table)"
              >
                {{ table.schema }}.{{ table.name }}
              </option>
            </select>
          </label>
          <label>
            Key field to reference
            <select
              v-model="activeRelationship.referencedColumn"
              @change="clearRelationshipFeedback"
              :disabled="disabled || applyingRelationship"
            >
              <option value="" disabled>Choose a field</option>
              <option
                v-for="field in relationshipTarget?.columns || []"
                :key="field"
                :value="field"
              >
                {{ field }} —
                {{ relationshipTarget ? referenceLabel(relationshipTarget, field) : '' }}
              </option>
            </select>
          </label>
        </div>
        <div v-if="pendingRelationship" class="diagram-relationship-actions">
          <button
            v-if="pendingRelationship.referencedTable"
            type="button"
            class="button secondary small"
            :disabled="disabled || applyingRelationship"
            @click="reversePendingRelationship"
          >
            Reverse direction
          </button>
          <button
            type="button"
            class="button secondary small"
            :disabled="disabled || applyingRelationship"
            @click="cancelPendingRelationship"
          >
            Cancel
          </button>
          <button
            type="button"
            class="button small"
            :disabled="disabled || applyingRelationship || !relationshipReady"
            @click="applyPendingRelationship"
          >
            {{ applyingRelationship ? 'Applying…' : 'Apply relationship' }}
          </button>
        </div>
        <div v-if="targetNeedsKey && relationshipTarget" class="diagram-relationship-recovery">
          <span
            >A primary key identifies a row. A unique key makes this field safe to reference without
            replacing the table’s primary key.</span
          >
          <button
            type="button"
            class="button secondary small"
            :disabled="disabled || applyingRelationship"
            @click="
              emit('prepareReference', relationshipTarget, activeRelationship!.referencedColumn)
            "
          >
            Prepare unique key for {{ activeRelationship?.referencedColumn }}
          </button>
        </div>
        <div
          v-else-if="relationshipError && relationshipSource && relationshipTarget"
          class="diagram-relationship-recovery"
        >
          <span>{{ relationshipFailureHint }}</span>
          <button
            v-if="/primary key|unique|index/i.test(relationshipError)"
            type="button"
            class="button secondary small"
            :disabled="disabled || applyingRelationship"
            @click="emit('editTable', relationshipTarget)"
          >
            Edit {{ relationshipTarget.name }} keys
          </button>
        </div>
        <div v-if="selectedRelationship" class="diagram-relationship-actions">
          <button
            type="button"
            class="button secondary small"
            :disabled="disabled || applyingRelationship"
            @click="cancelPendingRelationship"
          >
            Cancel
          </button>
          <button
            type="button"
            class="button secondary small diagram-relationship-remove"
            :disabled="disabled || applyingRelationship"
            @click="removeSelectedRelationship"
          >
            Remove
          </button>
          <button
            type="button"
            class="button small"
            :disabled="!canApplyRelationshipDraft"
            @click="applyRelationshipEdit"
          >
            {{ applyingRelationship ? 'Applying…' : 'Apply changes' }}
          </button>
        </div>
      </section>

      <main
        ref="viewport"
        class="schema-diagram-viewport"
        :class="{ panning: moving?.kind === 'pan' }"
        aria-label="Interactive schema diagram canvas"
        @pointerdown="startPan"
        @pointermove="movePointer"
        @pointerup="stopPointer"
        @pointercancel="stopPointer"
        @wheel.prevent="zoomAt"
      >
        <div v-if="loading" class="schema-diagram-empty" role="status">
          <span class="loader" />
          <p>Reading table structure…</p>
        </div>
        <div v-else-if="!tables.length" class="schema-diagram-empty">
          <AppIcon name="table" :size="26" />
          <h3>{{ editable ? 'No tables yet' : 'No accessible tables' }}</h3>
          <p>
            {{
              editable
                ? 'Add a table to start building this database.'
                : 'Refresh the database explorer after granting access to tables.'
            }}
          </p>
        </div>
        <div v-else-if="!visibleTables.length" class="schema-diagram-empty">
          <AppIcon name="search" :size="25" />
          <h3>No matching tables</h3>
          <p>Try a different table or column name.</p>
        </div>
        <div
          v-else
          class="schema-diagram-plane"
          :style="{
            width: planeSize.width + 'px',
            height: planeSize.height + 'px',
            transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
          }"
        >
          <svg
            class="schema-diagram-edges"
            :width="planeSize.width"
            :height="planeSize.height"
            :viewBox="`0 0 ${planeSize.width} ${planeSize.height}`"
            aria-label="Foreign key relationships"
          >
            <defs>
              <marker
                id="diagram-arrow"
                viewBox="0 0 10 10"
                refX="8"
                refY="5"
                markerWidth="6"
                markerHeight="6"
                orient="auto-start-reverse"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" />
              </marker>
            </defs>
            <path
              v-for="edge in edges"
              :key="edge.id"
              :d="edge.path"
              class="schema-diagram-edge"
              :class="{
                selected: isSelectedRelationship(edge.relationship),
                editable,
                emphasized:
                  hoveredTable === edge.sourceKey ||
                  hoveredTable === edge.targetKey ||
                  activeTable === edge.sourceKey ||
                  activeTable === edge.targetKey,
                subdued:
                  !!(hoveredTable || activeTable) &&
                  hoveredTable !== edge.sourceKey &&
                  hoveredTable !== edge.targetKey &&
                  activeTable !== edge.sourceKey &&
                  activeTable !== edge.targetKey,
              }"
              :tabindex="editable ? 0 : -1"
              role="button"
              :aria-label="editable ? `${edge.label}. Click to edit relationship.` : edge.label"
              :aria-disabled="!editable"
              @click.stop="selectRelationship(edge.relationship)"
              @keydown.enter.stop="selectRelationship(edge.relationship)"
              @keydown.space.prevent.stop="selectRelationship(edge.relationship)"
            >
              <title>{{ edge.label }}</title>
            </path>
            <path
              v-if="pendingRelationship"
              :d="relationshipPath(pendingRelationship)"
              class="schema-diagram-edge pending"
              aria-label="Pending relationship"
            />
            <path
              v-if="moving?.kind === 'link'"
              :d="linkDragPath(moving)"
              class="schema-diagram-edge pending dragging"
            />
          </svg>

          <article
            v-for="table in visibleTables"
            :key="tableKey(table)"
            class="schema-diagram-table"
            :data-diagram-node-key="tableKey(table)"
            :class="{
              'is-current':
                selectedTable?.schema === table.schema && selectedTable.name === table.name,
              'is-active': activeTable === tableKey(table),
              'is-related': isRelated(table),
            }"
            :style="{
              left: positionOf(table).x + 'px',
              top: positionOf(table).y + 'px',
              '--schema-accent': schemaColor(table.schema),
            }"
            @mouseenter="hoveredTable = tableKey(table)"
            @mouseleave="hoveredTable = null"
          >
            <button
              type="button"
              class="schema-diagram-table-heading"
              :aria-label="`Select ${table.schema}.${table.name}`"
              @pointerdown.stop="startNodeDrag($event, table)"
              @click="focusTable(table)"
            >
              <span class="schema-table-grip" aria-hidden="true"
                ><i /><i /><i /><i /><i /><i
              /></span>
              <span class="schema-table-heading-copy">
                <small>{{ table.schema }}</small>
                <strong>{{ table.name }}</strong>
              </span>
              <span class="schema-table-kind">{{
                table.kind.toLowerCase().includes('view') ? 'VIEW' : 'TABLE'
              }}</span>
            </button>
            <div class="schema-diagram-columns">
              <div
                v-for="column in table.columns.slice(0, displayedColumnCount(table))"
                :key="column"
                class="schema-diagram-column"
                :class="{
                  'schema-diagram-column-linkable': editable && table.kind === 'BASE TABLE',
                  'is-link-source':
                    moving?.kind === 'link' &&
                    moving.sourceKey === tableKey(table) &&
                    moving.sourceColumn === column,
                  'is-reference-ready':
                    (moving?.kind === 'link' || pendingRelationship) &&
                    ['primary', 'unique', 'indexed'].includes(referenceKind(table, column)),
                  'is-reference-hover': linkHover?.table === table && linkHover.column === column,
                  'is-pending-source':
                    pendingRelationship?.foreignSchema === table.schema &&
                    pendingRelationship.foreignTable === table.name &&
                    pendingRelationship.foreignColumn === column,
                  'is-pending-target':
                    activeRelationship?.referencedSchema === table.schema &&
                    activeRelationship.referencedTable === table.name &&
                    activeRelationship.referencedColumn === column,
                  'is-reference-needs-key':
                    referenceKind(table, column) === 'needs-key' &&
                    ((linkHover?.table === table && linkHover.column === column) ||
                      (activeRelationship?.referencedSchema === table.schema &&
                        activeRelationship.referencedTable === table.name &&
                        activeRelationship.referencedColumn === column)),
                }"
                :title="
                  editable
                    ? `${table.name}.${column}: ${referenceLabel(table, column)}. Click to choose a relationship field.`
                    : column
                "
                :data-diagram-table-key="tableKey(table)"
                :data-diagram-column-name="column"
                @pointerdown.stop="startFieldLink($event, table, column, connectionSide($event))"
                @click.stop="clickField(table, column)"
              >
                <span class="schema-column-mark">
                  <AppIcon v-if="primaryKeys(table).includes(column)" name="key" :size="13" />
                  <AppIcon
                    v-else-if="['unique', 'indexed'].includes(referenceKind(table, column))"
                    name="check"
                    :size="13"
                  />
                  <AppIcon v-else-if="isForeignKey(table, column)" name="link" :size="13" />
                  <i v-else />
                </span>
                <span
                  class="schema-column-name"
                  :class="{ 'is-primary': primaryKeys(table).includes(column) }"
                  >{{ column }}</span
                >
                <span v-if="primaryKeys(table).includes(column)" class="schema-column-badge primary"
                  >PK</span
                >
                <span v-else-if="isForeignKey(table, column)" class="schema-column-badge foreign"
                  >FK</span
                >
                <span
                  v-else-if="referenceKind(table, column) === 'unique'"
                  class="schema-column-badge unique"
                  >UK</span
                >
                <span
                  v-else-if="referenceKind(table, column) === 'indexed'"
                  class="schema-column-badge unique"
                  title="Eligible reference index"
                  >IDX</span
                >
                <template v-if="editable && table.kind === 'BASE TABLE'">
                  <button
                    v-for="side in connectionSides"
                    :key="`input-${side}`"
                    type="button"
                    class="schema-column-handle input"
                    :class="[
                      `handle-${side}`,
                      { 'needs-key': referenceKind(table, column) === 'needs-key' },
                    ]"
                    :data-diagram-handle="'input'"
                    :data-diagram-handle-side="side"
                    :aria-label="`Reference ${table.name}.${column}: ${referenceLabel(table, column)}`"
                    :title="`Reference ${column}: ${referenceLabel(table, column)}`"
                    :disabled="disabled"
                    @pointerdown.stop="startFieldLink($event, table, column, side)"
                  />
                  <button
                    v-for="side in connectionSides"
                    :key="`output-${side}`"
                    type="button"
                    class="schema-column-handle output"
                    :class="`handle-${side}`"
                    :data-diagram-handle="'output'"
                    :data-diagram-handle-side="side"
                    :aria-label="`Output point for ${table.name}.${column}`"
                    :title="`Drag to connect ${column}`"
                    :disabled="disabled"
                    @pointerdown.stop="startFieldLink($event, table, column, side)"
                  />
                </template>
              </div>
              <button
                v-if="
                  expandedTables.has(tableKey(table)) ||
                  table.columns.length > displayedColumnCount(table)
                "
                type="button"
                class="schema-column-more"
                :aria-label="`${expandedTables.has(tableKey(table)) ? 'Hide' : 'Show'} all fields for ${table.schema}.${table.name}`"
                @click="toggleTableFields(table)"
              >
                {{
                  expandedTables.has(tableKey(table))
                    ? 'Show fewer fields'
                    : `and ${table.columns.length - displayedColumnCount(table)} more ${table.columns.length - displayedColumnCount(table) === 1 ? 'field' : 'fields'}`
                }}
              </button>
              <p v-if="!table.columns.length" class="schema-columns-empty">No column metadata</p>
            </div>
            <footer class="schema-diagram-table-footer">
              <span
                >{{ table.columns.length }}
                {{ table.columns.length === 1 ? 'column' : 'columns' }}</span
              >
              <div class="schema-diagram-table-actions">
                <button
                  v-if="editable && table.kind === 'BASE TABLE'"
                  type="button"
                  :aria-label="`Edit ${table.schema}.${table.name} details`"
                  :disabled="disabled"
                  @click="emit('editTable', table)"
                >
                  Edit details
                </button>
                <button
                  type="button"
                  :aria-label="`Open ${table.schema}.${table.name}`"
                  @click="openTable(table)"
                >
                  Open <AppIcon name="arrow" :size="13" />
                </button>
              </div>
            </footer>
          </article>
        </div>
      </main>

      <footer class="schema-diagram-footer">
        <span>Scroll to zoom · Use Fit to bring all visible tables into view</span>
        <span
          >{{ visibleTables.length }} of {{ tables.length }} tables <i /> {{ edges.length }} visible
          relationships</span
        >
      </footer>
    </div>
  </dialog>
</template>
