<script setup lang="ts">
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
const { rowFilter, tab, tableTabs, activeView, activateQuery, runQuery } = workspaceTabs
const editor = ref<'insert' | 'delete' | 'schema' | null>(null)
const schemaAction = ref<'execute' | 'createDatabase'>('execute')
const schemaTargetTable = ref<TableInfo | null>(null)
const diagramTableEditor = ref<TableInfo | null | undefined>()
const diagramSuggestedUniqueColumn = ref<string>()
const diagramMutationBusy = ref(false)
const editingCell = ref<{
  row: Record<string, unknown>
  column: ColumnInfo
} | null>(null)
const editable = computed(
  () =>
    !!session.value &&
    session.value.provider !== 'demo' &&
    !session.value.readOnly &&
    providerOptions(session.value.provider).editing &&
    !workspaceTabs.queryActive.value &&
    selectedTable.value?.kind === 'BASE TABLE',
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
    (session.value?.provider === 'postgres' ||
      !!data.value?.columns.some((column) => column.primaryKey)),
)
const toast = useToast()
const settingsOpen = ref(false)
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
      run: async () => {
        await workspaceTabs.openTable(table)
        tab.value = 'structure'
      },
    },
  ]
  if (schemaEditable.value && table.kind === 'BASE TABLE')
    actions.push({
      label: 'Edit table details',
      disabled: disabled || diagramMutationBusy.value,
      run: () => openDiagramTableEditor(table),
    })
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
function rowContext(event: MouseEvent, index: number, column?: string) {
  const row = data.value?.rows[index]
  if (!row) return
  selectedRow.value = index
  const actions: ContextAction[] = [
    { label: 'Copy row as JSON', run: () => copy(row) },
    { label: 'Export current page', run: exportPage },
  ]
  if (column)
    actions.unshift(
      { label: 'Inspect cell', run: () => editCell(index, column) },
      { label: 'Copy cell value', run: () => copy(row[column]) },
    )
  if (editable.value)
    actions.push(
      {
        label: 'Insert record',
        run: () => {
          editor.value = 'insert'
        },
      },
      {
        disabled: !recordEditable.value,
        label: 'Delete row…',
        run: () => {
          editor.value = 'delete'
        },
      },
    )
  const number = page.value * pageSize.value + index + 1
  showContext(event, column ? `${column} · Row ${number}` : `Row ${number}`, actions)
}
async function mutationSaved() {
  const savedAction = editor.value === 'schema' ? schemaAction.value : null
  editor.value = null
  selectedRow.value = null
  workspaceTabs.invalidate()
  toast.show(
    savedAction === 'createDatabase'
      ? 'Database created successfully.'
      : savedAction === 'execute'
        ? 'Database change applied successfully.'
        : 'Change applied successfully.',
  )
  explorerKey.value++
  await db.refreshExplorer()
}
function openSchemaEditor(
  action: 'execute' | 'createDatabase' = 'execute',
  options: { table?: TableInfo | null } = {},
) {
  schemaAction.value = action
  schemaTargetTable.value = Object.hasOwn(options, 'table')
    ? (options.table ?? null)
    : selectedTable.value
  editor.value = 'schema'
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
  selectedRow.value === null ? null : data.value?.rows[selectedRow.value] || null,
)
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
watch(rowFilter, () => {
  selectedRow.value = null
})
function editCell(index: number, name: string) {
  selectedRow.value = index
  const row = data.value?.rows[index]
  const column = data.value?.columns.find((column) => column.name === name)
  if (row && column) editingCell.value = { row, column }
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
      <div class="header-branding">
        <a href="/" class="brand" aria-label="DBsaur home"
          ><span class="brand-icon"><AppIcon name="database" :size="20" /></span>DB<span
            class="brand-db"
            >saur</span
          ></a
        >
      </div>
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
          <AppIcon name="disconnect" :size="19" />
        </button>
      </div>
    </header>
    <div
      class="app-body"
      :class="{ disconnected: !session, 'resizing-sidebar': sidebar.resizing.value }"
      :style="{ '--sidebar-width': sidebar.width.value + 'px' }"
    >
      <aside v-if="session" id="database-sidebar" class="sidebar" aria-label="Database explorer">
        <div class="connection-card">
          <div class="connection-summary">
            <span class="provider-mark"><AppIcon name="database" :size="19" /></span>
            <div>
              <strong>{{
                session?.allDatabases ? session.host : session?.database || 'No connection'
              }}</strong
              ><span>{{
                session
                  ? session.provider === 'demo'
                    ? 'Sample workspace'
                    : session.allDatabases
                      ? `${providers.find((p) => p.id === session?.provider)?.name ?? session.provider} instance`
                      : `${providers.find((p) => p.id === session?.provider)?.name ?? session.provider} / ${session.host}`
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
                <AppIcon name="diagram" :size="17" />
                <span>Diagram</span>
              </button>
              <button
                v-if="session.allDatabases && !session.readOnly && session.provider !== 'demo'"
                class="icon-button database-create-button"
                title="Create database"
                aria-label="Create database"
                :disabled="connecting || switching || tablesLoading || busy"
                @click="openSchemaEditor('createDatabase')"
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
        <div class="table-heading">
          <div class="view-toolbar" role="group" aria-label="Table controls">
            <div class="tabs" role="tablist" aria-label="Table view">
              <button
                id="data-tab"
                role="tab"
                :aria-selected="tab === 'data'"
                :class="{ active: tab === 'data' }"
                @click="tab = 'data'"
                @keydown.right="tab = 'structure'"
              >
                <AppIcon name="table" :size="15" />{{
                  workspaceTabs.queryActive.value ? 'Results' : 'Data'
                }}<span v-if="data">{{ data.rows.length }}</span></button
              ><button
                id="structure-tab"
                role="tab"
                :aria-selected="tab === 'structure'"
                :class="{ active: tab === 'structure' }"
                @click="tab = 'structure'"
                @keydown.left="tab = 'data'"
              >
                <AppIcon name="grid" :size="15" />Structure<span v-if="data">{{
                  data.columns.length
                }}</span>
              </button>
            </div>
            <label v-if="tab === 'data'" class="search-box row-search"
              ><AppIcon name="search" :size="14" /><input
                v-model="rowFilter"
                placeholder="Filter this page…"
                aria-label="Filter rows on this page" /><button
                v-if="rowFilter"
                class="icon-button"
                title="Clear filter"
                aria-label="Clear filter"
                @click="rowFilter = ''"
              >
                <AppIcon name="close" :size="12" /></button
            ></label>
            <div class="table-heading-actions">
              <button
                v-if="recordEditable && record && tab === 'data'"
                class="button secondary small delete-row-button"
                :disabled="busy || switching"
                @click="editor = 'delete'"
              >
                Delete row
              </button>
              <button
                class="icon-button"
                title="Refresh rows"
                aria-label="Refresh rows"
                :disabled="
                  busy ||
                  switching ||
                  (workspaceTabs.queryActive.value ? !activeQuery : !selectedTable)
                "
                @click="db.loadRows"
              >
                <AppIcon name="refresh" :size="17" :class="{ spinning: busy }" /></button
              ><button
                class="icon-button"
                title="Export current page as JSON"
                aria-label="Export current page as JSON"
                :disabled="!data"
                @click="exportPage"
              >
                <AppIcon name="download" :size="17" />
              </button>
            </div>
            <button
              v-if="activeQuery"
              class="text-button back-to-table"
              :disabled="busy"
              @click="workspaceTabs.backToTable"
            >
              Back to table
            </button>
            <span v-if="session.readOnly" class="read-only-label">Read only</span>
            <div v-if="session.provider !== 'demo' && !session.readOnly" class="mutation-toolbar">
              <button
                v-if="editable"
                class="button secondary small"
                :disabled="busy || switching"
                @click="editor = 'insert'"
              >
                Insert record
              </button>
              <button
                v-if="
                  schemaEditable &&
                  selectedTable?.kind === 'BASE TABLE' &&
                  !workspaceTabs.queryActive.value
                "
                class="button secondary small"
                :disabled="busy || switching || diagramMutationBusy"
                @click="openDiagramTableEditor(selectedTable)"
              >
                Edit table details
              </button>
              <button
                class="button secondary small"
                :disabled="busy || switching"
                @click="openSchemaEditor()"
              >
                Database changes
              </button>
            </div>
          </div>
        </div>
        <div id="table-grid-region" class="data-body">
          <section
            :key="activeView"
            class="data-content"
            role="tabpanel"
            :aria-labelledby="tab === 'data' ? 'data-tab' : 'structure-tab'"
            :aria-busy="busy"
          >
            <div v-if="busy" class="content-empty" role="status">
              <span class="loader" />
              <p>Reading your table…</p>
            </div>
            <template v-else-if="data"
              ><DataGrid
                v-if="tab === 'data'"
                :data="data"
                :sort="sort"
                :descending="descending"
                :filter="rowFilter"
                :sortable="!activeQuery"
                :selected-row="selectedRow"
                @select="selectedRow = $event"
                @edit="editCell"
                @sort="db.sortBy"
                @context="rowContext"
              />
              <div v-else class="grid-scroll">
                <table class="structure-grid">
                  <thead>
                    <tr>
                      <th>Column</th>
                      <th>Type</th>
                      <th>Nullable</th>
                      <th>Key</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr v-for="column in data.columns" :key="column.name">
                      <td>{{ column.name }}</td>
                      <td>{{ column.dataType }}</td>
                      <td>{{ column.nullable ? 'Yes' : 'No' }}</td>
                      <td>
                        <span v-if="column.primaryKey" class="key-label"
                          ><AppIcon name="key" :size="13" />Primary key</span
                        ><span v-else class="muted">—</span>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div></template
            >
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
            ><span class="page-number">{{ page + 1 }}</span
            ><button
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
      :table="selectedTable"
      :editable="recordEditable"
      :readonly-reason="
        editable && !recordEditable
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
      :table="editor === 'schema' ? schemaTargetTable : selectedTable"
      :columns="data?.columns || []"
      :row="record"
      :database="session.database"
      :instance="session.allDatabases"
      :initial-action="schemaAction"
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
