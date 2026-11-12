<script setup lang="ts">
import { ref, shallowRef, computed, watch, nextTick, useTemplateRef } from 'vue'
import { useToast } from '../composables/useToast'
import { useDatabase } from '../composables/useDatabase'
import { useWorkspaceTabs } from '../composables/useWorkspaceTabs'
import { useSidebarResize } from '../composables/useSidebarResize'
import AppIcon from './AppIcon.vue'
import AppMenuBar from './AppMenuBar.vue'
import AboutDialog from './AboutDialog.vue'
import ShortcutsDialog from './ShortcutsDialog.vue'
import CellEditor from './CellEditor.vue'
import ConnectionForm from './ConnectionForm.vue'
import ContextMenu from './ContextMenu.vue'
import DatabaseExplorer from './DatabaseExplorer.vue'
import DatabaseLauncher from './DatabaseLauncher.vue'
import DatabaseSelect from './DatabaseSelect.vue'
import DatabaseSettings from './DatabaseSettings.vue'
import DataGrid from './DataGrid.vue'
import MutationDialog from './MutationDialog.vue'
import QueryEditor from './QueryEditor.vue'
import SchemaDiagram from './SchemaDiagram.vue'
import SchemaTableEditor from './SchemaTableEditor.vue'
import TableStructureDialog from './TableStructureDialog.vue'

import { providerOptions, quoteIdentifier } from '../providers'
import type {
  ColumnInfo,
  ConnectionInput,
  ContextAction,
  RelationshipChange,
  TableInfo,
} from '../../shared/database'
const db = useDatabase()
const workspaceTabs = useWorkspaceTabs(db)
const { tableTabs, activeView, activateQuery, runQuery } = workspaceTabs
const editor = ref<'delete' | 'createDatabase' | null>(null)
const structureTable = ref<TableInfo | null>(null)
const diagramTableEditor = ref<TableInfo | null | undefined>()
const diagramSuggestedUniqueColumn = ref<string>()
const diagramMutationBusy = ref(false)
const editingCell = ref<{
  row: Record<string, unknown>
  column: ColumnInfo
  table: TableInfo | null
} | null>(null)
const mutationTable = computed<TableInfo | null>(() => {
  if (!workspaceTabs.queryActive.value) return selectedTable.value
  const source = data.value?.editing
  return source
    ? {
        schema: source.schema,
        name: source.table,
        kind: 'BASE TABLE',
        columns: Object.values(source.columns),
      }
    : null
})
const editable = computed(
  () =>
    !!session.value &&
    session.value.provider !== 'demo' &&
    !session.value.readOnly &&
    providerOptions(session.value.provider).editing &&
    mutationTable.value?.kind === 'BASE TABLE',
)
const schemaEditable = computed(
  () =>
    !!session.value &&
    session.value.provider !== 'demo' &&
    !session.value.readOnly &&
    providerOptions(session.value.provider).editing,
)
const recordEditable = computed(
  () =>
    editable.value &&
    (!!data.value?.editing ||
      session.value?.provider === 'postgres' ||
      !!data.value?.columns.some((column) => column.primaryKey)),
)
const toast = useToast()
const settingsOpen = ref(false)
const aboutOpen = ref(false)
const shortcutsOpen = ref(false)
const schemaDiagramOpen = ref(false)
const context = shallowRef<{
  x: number
  y: number
  label: string
  actions: ContextAction[]
} | null>(null)
function showContext(event: MouseEvent, label: string, actions: ContextAction[]) {
  context.value = { x: event.clientX, y: event.clientY, label, actions }
}
async function copy(value: unknown) {
  try {
    await navigator.clipboard.writeText(
      typeof value === 'string' ? value : JSON.stringify(value, null, 2),
    )
    toast.show('Copied to clipboard.')
  } catch {
    toast.show('Clipboard access failed. Check your browser permissions.', 'error')
  }
}
function workspaceContext(event: MouseEvent) {
  showContext(event, 'Workspace', [
    {
      label: 'Refresh explorer',
      disabled: !session.value || busy.value || switching.value,
      run: db.refreshExplorer,
    },
    {
      label: 'Database settings',
      disabled: session.value?.provider !== 'postgres' || switching.value,
      run: () => {
        settingsOpen.value = true
      },
    },
  ])
}
function tableContext(event: MouseEvent, table: TableInfo) {
  const disabled = busy.value || switching.value
  const actions: ContextAction[] = [
    { label: 'Browse table', disabled, run: () => workspaceTabs.openTable(table) },
    {
      label: 'View structure',
      disabled,
      run: () => {
        structureTable.value = table
      },
    },
  ]
  if (schemaEditable.value && table.kind === 'BASE TABLE')
    actions.push({
      label: 'Edit table schema',
      disabled: disabled || diagramMutationBusy.value,
      run: () => openDiagramTableEditor(table),
    })
  if (activeView.value === workspaceTabs.key(table)) actions.push(...pageContextActions())
  actions.push(
    {
      label: 'Copy table name',
      run: () =>
        copy(
          `${quoteIdentifier(table.schema, session.value?.provider)}.${quoteIdentifier(table.name, session.value?.provider)}`,
        ),
    },
    { label: 'Refresh explorer', disabled, run: db.refreshExplorer },
  )
  showContext(event, table.name, actions)
}
function schemaContext(event: MouseEvent, schema: string) {
  showContext(event, schema, [
    { label: 'Copy schema name', run: () => copy(schema) },
    { label: 'Refresh explorer', disabled: switching.value || busy.value, run: db.refreshExplorer },
  ])
}
function pageContextActions(): ContextAction[] {
  const disabled = busy.value || switching.value || connecting.value
  const actions: ContextAction[] = [
    {
      label: activeQuery.value ? 'Refresh query results' : 'Refresh rows',
      disabled:
        disabled || !(workspaceTabs.queryActive.value ? activeQuery.value : selectedTable.value),
      run: db.loadRows,
    },
    { label: 'Export current page as JSON', disabled: disabled || !data.value, run: exportPage },
  ]
  if (activeQuery.value && selectedTable.value)
    actions.push({ label: 'Back to table', disabled, run: workspaceTabs.backToTable })
  return actions
}
function dataContext(event: MouseEvent) {
  showContext(
    event,
    workspaceTabs.queryActive.value ? 'Query results' : (selectedTable.value?.name ?? 'Table'),
    pageContextActions(),
  )
}
function rowContext(event: MouseEvent, index: number, column?: string) {
  const row = data.value?.rows[index]
  if (!row) return
  selectedRow.value = index
  const actions: ContextAction[] = [
    { label: 'Copy row as JSON', run: () => copy(row) },
    ...pageContextActions(),
  ]
  if (column)
    actions.unshift(
      { label: 'Inspect cell', run: () => editCell(index, column) },
      { label: 'Copy cell value', run: () => copy(row[column]) },
    )
  if (editable.value)
    actions.push({
      disabled: !recordEditable.value || busy.value || switching.value || connecting.value,
      label: 'Delete row…',
      run: () => {
        editor.value = 'delete'
      },
    })
  const number = page.value * pageSize.value + index + 1
  showContext(event, column ? `${column} · Row ${number}` : `Row ${number}`, actions)
}
async function mutationSaved() {
  const createdDatabase = editor.value === 'createDatabase'
  editor.value = null
  selectedRow.value = null
  workspaceTabs.invalidate()
  toast.show(createdDatabase ? 'Database created successfully.' : 'Row deleted successfully.')
  explorerKey.value++
  await db.refreshExplorer()
}
function openDiagramTableEditor(table: TableInfo | null, suggestedUniqueColumn?: string) {
  diagramSuggestedUniqueColumn.value = suggestedUniqueColumn
  diagramTableEditor.value = table
}
async function diagramTableSaved() {
  const created = diagramTableEditor.value === null
  diagramTableEditor.value = undefined
  workspaceTabs.invalidate()
  toast.show(created ? 'Table created successfully.' : 'Table details updated successfully.')
  explorerKey.value++
  await db.refreshExplorer()
}
async function applyDiagramRelationship(relationship: RelationshipChange) {
  const currentSession = session.value
  if (!currentSession || !schemaEditable.value)
    throw new Error('Schema changes are not available for this connection.')
  if (diagramMutationBusy.value || busy.value || switching.value)
    throw new Error('Wait for the current database operation to finish.')

  diagramMutationBusy.value = true
  try {
    const quote = (identifier: string) => quoteIdentifier(identifier, currentSession.provider)
    const constraint = `fk_${crypto.randomUUID().replaceAll('-', '').slice(0, 24)}`
    const sql = `ALTER TABLE ${quote(relationship.foreignSchema)}.${quote(relationship.foreignTable)}\n  ADD CONSTRAINT ${quote(constraint)} FOREIGN KEY (${quote(relationship.foreignColumn)})\n  REFERENCES ${quote(relationship.referencedSchema)}.${quote(relationship.referencedTable)} (${quote(relationship.referencedColumn)});`
    await db.applyMutation({ action: 'execute', sql })
    await refreshDiagramSchema('Relationship created successfully.')
  } finally {
    diagramMutationBusy.value = false
  }
}
function relationshipDropAction(constraintName: string, provider: string) {
  return provider === 'mysql' || provider === 'mariadb'
    ? `DROP FOREIGN KEY ${quoteIdentifier(constraintName, provider)}`
    : `DROP CONSTRAINT ${quoteIdentifier(constraintName, provider)}`
}
async function updateDiagramRelationship(current: RelationshipChange, next: RelationshipChange) {
  const currentSession = session.value
  if (!currentSession || !schemaEditable.value)
    throw new Error('Schema changes are not available for this connection.')
  if (!current.constraintName) throw new Error('The relationship name is unavailable.')
  if (diagramMutationBusy.value || busy.value || switching.value)
    throw new Error('Wait for the current database operation to finish.')

  diagramMutationBusy.value = true
  try {
    const provider = currentSession.provider
    const quote = (identifier: string) => quoteIdentifier(identifier, provider)
    const target = `${quote(current.foreignSchema)}.${quote(current.foreignTable)}`
    const addConstraint = `ADD CONSTRAINT ${quote(current.constraintName)} FOREIGN KEY (${quote(next.foreignColumn)}) REFERENCES ${quote(next.referencedSchema)}.${quote(next.referencedTable)} (${quote(next.referencedColumn)})`
    const sql = `ALTER TABLE ${target}\n  ${relationshipDropAction(current.constraintName, provider)},\n  ${addConstraint};`
    await db.applyMutation({ action: 'execute', sql })
    await refreshDiagramSchema('Relationship updated successfully.')
  } finally {
    diagramMutationBusy.value = false
  }
}
async function removeDiagramRelationship(relationship: RelationshipChange) {
  const currentSession = session.value
  if (!currentSession || !schemaEditable.value)
    throw new Error('Schema changes are not available for this connection.')
  if (!relationship.constraintName) throw new Error('The relationship name is unavailable.')
  if (diagramMutationBusy.value || busy.value || switching.value)
    throw new Error('Wait for the current database operation to finish.')

  diagramMutationBusy.value = true
  try {
    const quote = (identifier: string) => quoteIdentifier(identifier, currentSession.provider)
    const target = `${quote(relationship.foreignSchema)}.${quote(relationship.foreignTable)}`
    const sql = `ALTER TABLE ${target} ${relationshipDropAction(relationship.constraintName, currentSession.provider)};`
    await db.applyMutation({ action: 'execute', sql })
    await refreshDiagramSchema('Relationship removed successfully.')
  } finally {
    diagramMutationBusy.value = false
  }
}
async function refreshDiagramSchema(message: string) {
  workspaceTabs.invalidate()
  toast.show(message)
  explorerKey.value++
  await db.refreshExplorer()
}
const {
  providers,
  session,
  selectedTable,
  data,
  selectedRow,
  busy,
  connecting,
  connectionError,
  error,
  page,
  pageSize,
  sort,
  descending,
  activeQuery,
  databases,
  tables,
  tablesLoading,
  savedConnections,
  savingEnabled,
  switching,
} = db
const sidebar = useSidebarResize()
const explorerKey = ref(0)
const queryWorkspaceKey = ref(0)
watch(session, () => {
  settingsOpen.value = false
  schemaDiagramOpen.value = false
  structureTable.value = null
  diagramTableEditor.value = undefined
  context.value = null
  editingCell.value = null
  editor.value = null
  queryWorkspaceKey.value++
})
function tableTabId(table: TableInfo) {
  return 'table-' + encodeURIComponent(workspaceTabs.key(table))
}
async function focusTable(table: TableInfo) {
  await nextTick()
  document.getElementById(tableTabId(table))?.focus()
}
async function closeTable(table: TableInfo) {
  await workspaceTabs.closeTable(table)
  const active = tableTabs.value.find((item) => workspaceTabs.key(item) === activeView.value)
  if (active) await focusTable(active)
  else if (!tableTabs.value.length)
    document.querySelector<HTMLButtonElement>('.table-link')?.focus()
}
async function navigateTable(event: KeyboardEvent, index: number) {
  if (busy.value || switching.value || connecting.value) return
  if (event.key === 'Delete') {
    event.preventDefault()
    await closeTable(tableTabs.value[index]!)
    return
  }
  const count = tableTabs.value.length
  const target =
    event.key === 'ArrowRight'
      ? (index + 1) % count
      : event.key === 'ArrowLeft'
        ? (index + count - 1) % count
        : event.key === 'Home'
          ? 0
          : event.key === 'End'
            ? count - 1
            : null
  if (target === null) return
  event.preventDefault()
  const table = tableTabs.value[target]!
  await workspaceTabs.openTable(table)
  await focusTable(table)
}
watch(activeView, async () => {
  await nextTick()
  const active = tableTabs.value.find((item) => workspaceTabs.key(item) === activeView.value)
  if (active)
    document
      .getElementById(tableTabId(active))
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
})
const connectionDialog = useTemplateRef<HTMLDialogElement>('connectionDialog')
const initialSql = computed(() => {
  return selectedTable.value
    ? `SELECT * FROM ${quoteIdentifier(selectedTable.value.schema, session.value?.provider)}.${quoteIdentifier(selectedTable.value.name, session.value?.provider)};`
    : 'SELECT 1 AS result;'
})
const record = computed(() =>
  selectedRow.value === null
    ? null
    : (data.value?.editing?.originals[selectedRow.value] ?? data.value?.rows[selectedRow.value]) ||
      null,
)
const totalPages = computed(() => {
  if (data.value?.totalRows == null) return null
  const total = BigInt(data.value.totalRows)
  const count = (total + BigInt(pageSize.value) - 1n) / BigInt(pageSize.value)
  return (count || 1n).toLocaleString()
})
const jumpPage = ref('')
const maxJumpPage = computed(() => {
  if (data.value?.totalRows == null) return 100001
  const count =
    (BigInt(data.value.totalRows) + BigInt(pageSize.value) - 1n) / BigInt(pageSize.value)
  return Number(count > 100001n ? 100001n : count || 1n)
})
const jumpValid = computed(
  () =>
    /^\d+$/.test(jumpPage.value) &&
    Number(jumpPage.value) >= 1 &&
    Number(jumpPage.value) <= maxJumpPage.value,
)
async function goToPage() {
  if (!jumpValid.value || busy.value) return
  await db.changePage(Number(jumpPage.value) - 1)
  jumpPage.value = ''
}
watch([data, busy, switching], () => {
  context.value = null
})
async function connect(input: ConnectionInput | string) {
  if (await db.connect(input, typeof input === 'string' ? input : undefined)) {
    explorerKey.value++
    connectionDialog.value?.close()
  }
}
const sampleConnection: ConnectionInput = {
  provider: 'demo',
  host: '',
  port: 0,
  database: '',
  username: '',
  password: '',
  ssl: false,
  readOnly: true,
}
function exportPage() {
  if (!data.value) return
  const blob = new Blob([JSON.stringify(data.value.rows, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = `${activeQuery.value ? 'query-results' : selectedTable.value?.name || 'table'}-page-${page.value + 1}.json`
  link.click()
  toast.show('JSON export downloaded.')
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
function editCell(index: number, name: string) {
  selectedRow.value = index
  const row = data.value?.editing?.originals[index] ?? data.value?.rows[index]
  const column = data.value?.columns.find((column) => column.name === name)
  if (row && column)
    editingCell.value = {
      row,
      column: { ...column, name: data.value?.editing?.columns[name] ?? name },
      table: mutationTable.value,
    }
}
async function cellSaved() {
  editingCell.value = null
  workspaceTabs.invalidate()
  toast.show('Value saved.')
  await db.loadRows()
}
</script>
<template>
  <div class="workspace" @contextmenu.prevent="workspaceContext">
    <header class="app-header">
      <AppMenuBar
        :menus="[
          {
            label: 'File',
            actions: [
              { label: 'New connection…', run: () => connectionDialog?.showModal() },
              {
                label: 'Disconnect',
                disabled: !session || connecting || switching,
                run: db.disconnect,
              },
            ],
          },
          {
            label: 'View',
            actions: [
              { label: 'Reset explorer width', disabled: !session, run: sidebar.reset },
              {
                label: 'Database diagram',
                disabled: !session || connecting || switching || tablesLoading,
                run: () => {
                  schemaDiagramOpen = true
                },
              },
              {
                label: 'Refresh explorer',
                disabled: !session || connecting || switching || busy,
                run: db.refreshExplorer,
              },
            ],
          },
          {
            label: 'Help',
            actions: [
              {
                label: 'Keyboard shortcuts',
                run: () => {
                  shortcutsOpen = true
                },
              },
              {
                label: 'About DBsaur…',
                run: () => {
                  aboutOpen = true
                },
              },
            ],
          },
        ]"
      />
      <div v-if="session" class="header-actions">
        <button class="button secondary small" @click="connectionDialog?.showModal()">
          <AppIcon name="plus" :size="15" /><span>New connection</span>
        </button>
        <button
          class="icon-button"
          title="Disconnect"
          aria-label="Disconnect"
          :disabled="connecting || switching"
          @click="db.disconnect"
        >
          <AppIcon name="disconnect" :size="16" />
        </button>
      </div>
    </header>
    <ShortcutsDialog v-if="shortcutsOpen" @close="shortcutsOpen = false" />
    <AboutDialog v-if="aboutOpen" @close="aboutOpen = false" />
    <div
      class="app-body"
      :class="{ disconnected: !session, 'resizing-sidebar': sidebar.resizing.value }"
      :style="{ '--sidebar-width': sidebar.width.value + 'px' }"
    >
      <aside v-if="session" id="database-sidebar" class="sidebar" aria-label="Database explorer">
        <div class="connection-card">
          <div class="connection-summary">
            <span class="provider-mark"><AppIcon name="database" :size="16" /></span>
            <div>
              <strong>{{
                session?.allDatabases ? session.host : session?.database || 'No connection'
              }}</strong><span>{{
                session
                  ? session.provider === 'demo'
                    ? 'Sample workspace'
                    : session.allDatabases
                      ? `${providers.find((p) => p.id === session?.provider)?.name ?? session.provider} instance`
                      : `${providers.find((p) => p.id === session?.provider)?.name ?? session.provider} / ${session.host || 'Local file'}`
                  : 'Connect to get started'
              }}</span>
            </div>
          </div>
          <div v-if="session" class="database-select-row">
            <div class="database-select-controls">
              <DatabaseSelect
                :databases="session.allDatabases ? databases : [session.database]"
                :model-value="session.database"
                :disabled="connecting || switching || tablesLoading"
                @update:model-value="db.switchDatabase($event)"
              />
              <button
                v-if="session.provider === 'postgres'"
                class="icon-button database-settings-button"
                title="Database settings"
                aria-label="Database settings"
                :disabled="connecting || switching"
                @click="settingsOpen = true"
              >
                <AppIcon name="settings" :size="16" />
              </button>
            </div>
            <div class="database-view-actions">
              <button
                class="button secondary small database-diagram-button"
                title="Open database diagram"
                aria-label="Open database diagram"
                :disabled="connecting || switching || tablesLoading"
                @click="schemaDiagramOpen = true"
              >
                <AppIcon name="diagram" :size="14" />
                <span>Diagram</span>
              </button>
              <button
                v-if="session.allDatabases && !session.readOnly && session.provider !== 'demo'"
                class="icon-button database-create-button"
                title="Create database"
                aria-label="Create database"
                :disabled="connecting || switching || tablesLoading || busy"
                @click="editor = 'createDatabase'"
              >
                <AppIcon name="plus" :size="17" />
              </button>
            </div>
          </div>
        </div>
        <DatabaseExplorer
          v-if="session"
          :key="explorerKey"
          :tables="tables"
          :loading="tablesLoading"
          :active-database="session.database"
          :selected-table="selectedTable"
          :disabled="connecting || switching || busy || diagramMutationBusy"
          :can-create-table="schemaEditable"
          @choose="workspaceTabs.chooseDatabaseTable"
          @create-table="openDiagramTableEditor(null)"
          @refresh="db.refreshExplorer"
          @context="tableContext"
          @schema-context="schemaContext"
        />
        <div v-else class="sidebar-placeholder">
          <AppIcon name="table" :size="22" />
          <p>Your tables will<br />appear here.</p>
        </div>
      </aside>
      <div
        v-if="session"
        class="sidebar-resizer"
        role="separator"
        tabindex="0"
        aria-label="Resize database explorer"
        aria-controls="database-sidebar"
        aria-orientation="vertical"
        :aria-valuemin="sidebar.minimum.value"
        :aria-valuemax="sidebar.maximum.value"
        :aria-valuenow="sidebar.width.value"
        :aria-valuetext="sidebar.width.value + ' pixels'"
        title="Drag to resize. Arrow keys resize; double-click resets."
        @pointerdown="sidebar.start"
        @pointermove="sidebar.move"
        @pointerup="sidebar.stop"
        @pointercancel="sidebar.stop"
        @lostpointercapture="sidebar.stop"
        @keydown="sidebar.keydown"
        @dblclick="sidebar.reset"
      />
      <DatabaseLauncher
        v-if="!session"
        :saved-connections="savedConnections"
        :busy="connecting || db.testing.value"
        :error="connectionError"
        @reconnect="connect"
        @remove="db.deleteSavedConnection"
        @add="connectionDialog?.showModal()"
        @sample="connect(sampleConnection)"
      />
      <main v-else class="database-main">
        <QueryEditor
          :key="queryWorkspaceKey"
          :initial-sql="initialSql"
          :provider="session.provider"
          :busy="busy || switching"
          :active-query="activeQuery"
          :demo="session.provider === 'demo'"
          :tables="tables"
          :selected-table="selectedTable"
          @run="runQuery"
          @activate="activateQuery"
          @close="workspaceTabs.closeQuery"
        />
        <div v-if="tableTabs.length" class="table-tabs" role="tablist" aria-label="Open tables">
          <div
            v-for="(table, index) in tableTabs"
            :key="workspaceTabs.key(table)"
            class="table-tab"
            :class="{ active: activeView === workspaceTabs.key(table) }"
            role="presentation"
            @contextmenu.prevent.stop="tableContext($event, table)"
          >
            <button
              :id="tableTabId(table)"
              class="table-tab-label"
              role="tab"
              :aria-selected="activeView === workspaceTabs.key(table)"
              aria-controls="table-grid-region"
              :tabindex="
                activeView === workspaceTabs.key(table) ||
                (workspaceTabs.queryActive.value && index === 0)
                  ? 0
                  : -1
              "
              :disabled="busy || switching || connecting"
              :title="session.database + '.' + table.schema + '.' + table.name"
              @click="workspaceTabs.openTable(table)"
              @keydown="navigateTable($event, index)"
            >
              <AppIcon name="table" :size="14" />
              <span class="table-tab-title">{{ table.name }}</span>
            </button>
            <button
              class="query-tab-close"
              type="button"
              :disabled="busy || switching || connecting"
              :aria-label="'Close table ' + table.schema + '.' + table.name"
              title="Close table"
              @click="closeTable(table)"
            >
              <AppIcon name="close" :size="12" />
            </button>
          </div>
        </div>
        <div id="table-grid-region" class="data-body" @contextmenu.prevent.stop="dataContext">
          <section
            :key="activeView"
            class="data-content"
            role="region"
            :aria-label="workspaceTabs.queryActive.value ? 'Query results' : 'Table data'"
            :aria-busy="busy"
          >
            <div v-if="busy" class="content-empty" role="status">
              <span class="loader" />
              <p>Reading your table…</p>
            </div>
            <template v-else-if="data"
              ><DataGrid
                :data="data"
                :sort="sort"
                :descending="descending"
                :sortable="!activeQuery"
                :selected-row="selectedRow"
                @select="selectedRow = $event"
                @edit="editCell"
                @sort="db.sortBy"
                @context="rowContext"
              />
            </template>
            <div v-else class="content-empty">
              <AppIcon name="table" :size="32" />
              <h2>{{ error ? 'Unable to load this table' : 'Ready when you are' }}</h2>
              <p>
                {{
                  error
                    ? 'Check your connection and database permissions, then try again.'
                    : workspaceTabs.queryActive.value
                      ? 'Run this query to see its results.'
                      : 'Select a table from the explorer to open a tab.'
                }}
              </p>
              <button
                v-if="error"
                class="button secondary small"
                @click="selectedTable || activeQuery ? db.loadRows() : db.loadTables()"
              >
                Try again
              </button>
            </div>
          </section>
        </div>
        <footer v-if="data" class="pagination table-footer" aria-label="Table pagination">
          <span v-if="session.readOnly" class="read-only-label">Read only</span>
          <span
            >{{
              data.rows.length
                ? `${page * pageSize + 1}–${page * pageSize + data.rows.length}`
                : '0'
            }}
            {{ activeQuery ? 'result rows' : 'rows'
            }}<span class="pagination-detail"> · {{ data.columns.length }} columns</span></span
          >
          <div>
            <label
              >Rows per page<select
                :value="pageSize"
                aria-label="Rows per page"
                :disabled="busy"
                @change="db.changePageSize(Number(($event.target as HTMLSelectElement).value))"
              >
                <option :value="25">25</option>
                <option :value="50">50</option>
                <option :value="100">100</option>
                <option :value="500">500</option>
                <option :value="1000">1000</option>
              </select></label
            ><button
              class="icon-button previous"
              title="Previous page"
              aria-label="Previous page"
              :disabled="page === 0 || busy"
              @click="db.changePage(page - 1)"
            >
              <AppIcon name="chevron" :size="15" /></button
            ><span class="page-number" aria-live="polite"
              >Page {{ page + 1 }}<template v-if="totalPages"> of {{ totalPages }}</template></span
            >
            <form class="page-jump" @submit.prevent="goToPage">
              <input
                v-model="jumpPage"
                type="number"
                min="1"
                :max="maxJumpPage"
                step="1"
                aria-label="Go to page"
                placeholder="Page"
                :disabled="busy"
              />
              <button class="button secondary small" type="submit" :disabled="busy || !jumpValid">
                Go
              </button>
            </form>
            <button
              class="icon-button"
              title="Next page"
              aria-label="Next page"
              :disabled="!data.hasMore || busy"
              @click="db.changePage(page + 1)"
            >
              <AppIcon name="chevron" :size="15" />
            </button>
          </div>
        </footer>
      </main>
    </div>
    <ContextMenu
      v-if="context"
      :key="`${context.x}-${context.y}-${context.label}`"
      v-bind="context"
      @close="context = null"
    />
    <DatabaseSettings
      v-if="settingsOpen && session"
      :database="session.database"
      :provider="session.provider"
      @close="settingsOpen = false"
    />
    <SchemaDiagram
      v-if="schemaDiagramOpen && session"
      :database="session.database"
      :tables="tables"
      :selected-table="selectedTable"
      :loading="tablesLoading"
      :editable="schemaEditable"
      :disabled="connecting || switching || tablesLoading || busy || diagramMutationBusy"
      :apply-relationship="applyDiagramRelationship"
      :update-relationship="updateDiagramRelationship"
      :remove-relationship="removeDiagramRelationship"
      @choose="workspaceTabs.chooseDatabaseTable(session.database, $event)"
      @create-table="openDiagramTableEditor(null)"
      @edit-table="openDiagramTableEditor"
      @prepare-reference="(table, column) => openDiagramTableEditor(table, column)"
      @close="schemaDiagramOpen = false"
    />
    <TableStructureDialog
      v-if="structureTable && session"
      :table="structureTable"
      @close="structureTable = null"
    />
    <SchemaTableEditor
      v-if="session && diagramTableEditor !== undefined"
      :provider="session.provider"
      :database="session.database"
      :table="diagramTableEditor"
      :suggest-unique-column="diagramSuggestedUniqueColumn"
      :apply="db.applyMutation"
      @close="diagramTableEditor = undefined"
      @saved="diagramTableSaved"
    />
    <CellEditor
      v-if="editingCell"
      :column="editingCell.column"
      :row="editingCell.row"
      :table="editingCell.table"
      :provider="session?.provider ?? 'postgres'"
      :editable="recordEditable"
      :readonly-reason="
        workspaceTabs.queryActive.value && !data?.editing
          ? 'This result cannot be mapped to one editable base table. Use a direct single-table SELECT to edit records.'
          : editable && !recordEditable
            ? 'Updating records requires a primary key for this provider.'
            : undefined
      "
      :apply="db.applyMutation"
      @close="editingCell = null"
      @saved="cellSaved"
    />
    <MutationDialog
      v-if="editor && session"
      :mode="editor"
      :provider="session.provider"
      :table="mutationTable"
      :row="record"
      :database="session.database"
      :apply="db.applyMutation"
      @close="editor = null"
      @saved="mutationSaved"
    />
    <dialog ref="connectionDialog" class="connection-dialog" aria-label="Connect to a database">
      <button
        class="icon-button dialog-close"
        title="Close connection dialog"
        aria-label="Close connection dialog"
        @click="connectionDialog?.close()"
      >
        <AppIcon name="close" /></button
      ><ConnectionForm
        :providers="providers"
        :busy="connecting || db.testing.value"
        :test-connection="db.testConnection"
        :error="connectionError"
        :saved-connections="savedConnections"
        :saving-enabled="savingEnabled"
        :show-saved-connections="!!session"
        @reconnect="connect"
        @remove="db.deleteSavedConnection"
        @connect="connect"
      />
    </dialog>
  </div>
</template>
