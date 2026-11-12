<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { Mutation, TableInfo, TablePage } from '../../shared/database'
import { mongoFind, mongoAggregate } from '../utils/mongoQuery'
import JsonEditor from './JsonEditor.vue'
import AppIcon from './AppIcon.vue'
import AppMenuBar from './AppMenuBar.vue'
import MongoDocumentCard from './MongoDocumentCard.vue'
import MongoDocumentDialog from './MongoDocumentDialog.vue'

const props = defineProps<{
  table: TableInfo | null
  data: TablePage | null
  busy: boolean
  error: string
  page: number
  pageSize: number
  activeQuery: string | null
  readOnly: boolean
  apply: (mutation: Mutation) => Promise<{ affectedRows: number }>
}>()
const emit = defineEmits<{
  find: [query: string]
  refresh: []
  page: [page: number]
  pageSize: [size: number]
  export: []
  saved: []
}>()
const filter = ref('{}')
const sort = ref('{}')
const projection = ref('{}')
const localError = ref('')
const mode = ref('find')
const pipeline = ref('[]')
const editor = ref<{
  mode: 'insert' | 'replace' | 'delete'
  row?: Record<string, unknown>
  snapshot?: string
} | null>(null)
const canWrite = computed(
  () => !props.readOnly && !!props.table && !props.table.name.startsWith('system.'),
)
function editDisabledReason(index: number) {
  if (props.readOnly)
    return 'This connection is read only. Reconnect with read-only mode turned off.'
  if (props.table?.name.startsWith('system.')) return 'System collections cannot be edited.'
  if (applied.value?.pipeline) return 'Aggregation results cannot be edited. Use Find documents.'
  if (Object.keys(applied.value?.projection ?? {}).length)
    return 'Partial documents cannot be edited. Clear Projection and run Find documents.'
  if (!props.table) return 'Select a collection in the explorer before editing documents.'
  if (!props.data?.editing)
    return 'Editing metadata is unavailable. Restart DBsaur with the latest build, reconnect, and reload the collection.'
  if (typeof props.data.editing.originals[index]?.snapshot !== 'string')
    return 'This document is view-only. Editing requires an _id and a document no larger than 1 MiB of BSON.'
  return ''
}
function edit(mode: 'replace' | 'delete', row: Record<string, unknown>, index: number) {
  const snapshot = props.data?.editing?.originals[index]?.snapshot
  if (!props.busy && !editDisabledReason(index) && typeof snapshot === 'string')
    editor.value = { mode, row, snapshot }
}
function saved() {
  editor.value = null
  emit('saved')
}
const applied = computed(() => {
  try {
    const query = props.activeQuery ? JSON.parse(props.activeQuery) : null
    return query?.collection === props.table?.name ? query : null
  } catch {
    return null
  }
})
watch(
  () => [props.table?.name, props.activeQuery],
  () => {
    filter.value = JSON.stringify(applied.value?.filter ?? {}, null, 2)
    sort.value = JSON.stringify(applied.value?.sort ?? {})
    projection.value = JSON.stringify(applied.value?.projection ?? {})
    mode.value = applied.value?.pipeline ? 'aggregate' : 'find'
    pipeline.value = JSON.stringify(applied.value?.pipeline ?? [], null, 2)
    localError.value = ''
  },
  { immediate: true },
)
function find() {
  if (props.busy) return
  localError.value = ''
  try {
    emit(
      'find',
      mode.value === 'aggregate'
        ? mongoAggregate(props.table?.name ?? '', pipeline.value)
        : mongoFind(props.table?.name ?? '', filter.value, sort.value, projection.value),
    )
  } catch (cause) {
    localError.value = cause instanceof Error ? cause.message : 'Check the query.'
  }
}
function reset() {
  filter.value = sort.value = projection.value = '{}'
  pipeline.value = '[]'
  mode.value = 'find'
  find()
}
function identity(row: Record<string, unknown>, index: number) {
  return '_id' in row
    ? JSON.stringify(row._id)
    : `Document ${props.page * props.pageSize + index + 1}`
}
</script>
<template>
  <main class="database-main mongo-workspace">
    <section class="mongo-query query-space" aria-label="MongoDB query workspace">
      <div class="query-toolbar mongo-toolbar">
        <h1 class="mongo-title" :title="table?.name">{{ table?.name ?? 'Collections' }}</h1>
        <AppMenuBar
          :menus="[
            {
              label: 'Collection',
              actions: [
                {
                  label: 'Insert document…',
                  disabled: busy || !canWrite,
                  run: () => {
                    editor = { mode: 'insert' }
                  },
                },
                {
                  label: 'Refresh documents',
                  disabled: busy || !table,
                  run: () => emit('refresh'),
                },
                {
                  label: 'Export current page as JSON',
                  disabled: busy || !data,
                  run: () => emit('export'),
                },
              ],
            },
            {
              label: 'Query',
              actions: [{ label: 'Reset query', disabled: busy || !table, run: reset }],
            },
          ]"
        />
        <select v-model="mode" aria-label="Query mode" :disabled="busy || !table">
          <option value="find">Find documents</option>
          <option value="aggregate">Aggregation pipeline</option>
        </select>
        <details
          class="mongo-query-options"
          @keydown.esc="($event.currentTarget as HTMLDetailsElement).open = false"
        >
          <summary>
            Options<span v-if="sort.trim() !== '{}' || projection.trim() !== '{}'">
              · Modified</span
            >
          </summary>
          <div class="mongo-options-panel">
            <template v-if="mode === 'find'">
              <div class="mongo-field">
                <span>Sort</span>
                <JsonEditor
                  v-model="sort"
                  label="MongoDB sort"
                  :height="64"
                  :disabled="busy || !table"
                  @run="find"
                />
              </div>
              <div class="mongo-field">
                <span>Projection</span>
                <JsonEditor
                  v-model="projection"
                  label="MongoDB projection"
                  :height="64"
                  :disabled="busy || !table"
                  @run="find"
                />
              </div>
              <p class="field-help">
                Filter matches fields, for example { "status": "active", "age": { "$gte": 18 } }.
                JSON objects only. Sort: 1 ascending, -1 descending. Projection: 1 include, 0
                exclude. BSON values use Extended JSON, e.g. { "$oid": "507f1f77bcf86cd799439011" }.
              </p>
            </template>
            <p v-else class="field-help">
              A pipeline is an array of read-only stages. For totals by status, try [{ "$group": {
              "_id": "$status", "count": { "$sum": 1 } } }, { "$sort": { "count": -1 } }]. Results
              cannot be edited. Add a deterministic $sort for consistent pages.
            </p>
            <p v-if="!readOnly && data && !data.editing" class="field-help">
              To edit documents, use Find with an empty projection. Documents larger than 1 MiB of
              BSON are view-only.
            </p>
          </div>
        </details>
        <button type="button" class="button primary small" :disabled="busy || !table" @click="find">
          <AppIcon name="play" :size="14" />
          Run
        </button>
      </div>
      <form class="mongo-query-fields" @submit.prevent="find">
        <template v-if="mode === 'find'">
          <div class="mongo-field">
            <span>Filter</span
            ><JsonEditor
              v-model="filter"
              label="MongoDB filter"
              :height="64"
              :disabled="busy || !table"
              @run="find"
            />
          </div>
        </template>
        <div v-else class="mongo-field">
          <span>Pipeline</span
          ><JsonEditor
            v-model="pipeline"
            label="MongoDB aggregation pipeline"
            :height="100"
            :disabled="busy || !table"
            @run="find"
          />
        </div>
        <p v-if="localError || error" class="mongo-error" role="alert">{{ localError || error }}</p>
      </form>
    </section>
    <section class="mongo-documents" aria-label="Documents" :aria-busy="busy">
      <div v-if="busy" class="content-empty" role="status">
        <span class="loader" />
        <p>Reading documents…</p>
      </div>
      <div v-else-if="error" class="content-empty">
        <h2>Unable to read documents</h2>
        <p>Check the query and database permissions.</p>
        <button class="button secondary small" @click="emit('refresh')">Try again</button>
      </div>
      <div v-else-if="!data?.rows.length" class="content-empty">
        <h2>{{ table ? 'No documents found' : 'Choose a collection' }}</h2>
        <p>
          {{
            table
              ? 'Try a different filter, or reset to browse the collection.'
              : 'Select a collection in the explorer to browse its documents.'
          }}
        </p>
      </div>
      <MongoDocumentCard
        v-for="(row, index) in data?.rows ?? []"
        v-else
        :key="index"
        :row="row"
        :identity="identity(row, index)"
        :disabled="busy"
        :editable="!editDisabledReason(index)"
        :edit-disabled-reason="editDisabledReason(index)"
        @edit="edit('replace', row, index)"
        @delete="edit('delete', row, index)"
      />
    </section>
    <footer
      v-if="data"
      class="pagination table-footer mongo-pagination"
      aria-label="Document pagination"
    >
      <span v-if="readOnly" class="read-only-label">Read only</span>
      <span
        >{{
          data.rows.length ? `${page * pageSize + 1}–${page * pageSize + data.rows.length}` : '0'
        }}
        documents</span
      >
      <div>
        <label
          >Documents per page<select
            :value="pageSize"
            :disabled="busy"
            @change="emit('pageSize', Number(($event.target as HTMLSelectElement).value))"
          >
            <option :value="25">25</option>
            <option :value="50">50</option>
            <option :value="100">100</option>
          </select></label
        >
        <button
          class="icon-button previous"
          title="Previous page"
          aria-label="Previous page"
          :disabled="busy || page === 0"
          @click="emit('page', page - 1)"
        >
          <AppIcon name="chevron" :size="15" />
        </button>
        <span class="page-number" aria-live="polite">Page {{ page + 1 }}</span>
        <button
          class="icon-button"
          title="Next page"
          aria-label="Next page"
          :disabled="busy || !data.hasMore || (page + 1) * pageSize > 100000"
          @click="emit('page', page + 1)"
        >
          <AppIcon name="chevron" :size="15" />
        </button>
      </div>
    </footer>
    <MongoDocumentDialog
      v-if="editor && table"
      :table="table"
      :mode="editor.mode"
      :row="editor.row"
      :snapshot="editor.snapshot"
      :apply="apply"
      @close="editor = null"
      @saved="saved"
    />
  </main>
</template>
<style scoped>
.mongo-workspace {
  min-width: 0;
}
.mongo-title {
  font-size: 13px;
  font-weight: 600;
  margin: 0;
  margin-right: auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mongo-query {
  flex-shrink: 0;
}
.mongo-toolbar {
  padding: 6px 24px;
  flex-wrap: wrap;
  justify-content: flex-start;
  gap: 10px;
}
.mongo-query-fields {
  isolation: isolate;
  padding: 8px 24px 12px;
}
.mongo-query label,
.mongo-field {
  display: grid;
  gap: 6px;
  font-size: 12px;
  min-width: 0;
}
.mongo-query select {
  min-width: 0;
  max-width: 100%;
  background: var(--canvas);
  color: var(--text);
  border: 1px solid var(--raised);
  border-radius: 4px;
  padding: 5px;
  font-family: inherit;
  font-size: 12px;
}
.mongo-query select:focus-visible,
.mongo-query-options summary:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.mongo-query-options {
  position: relative;
  z-index: 1;
  font-size: 12px;
}
.mongo-query-options summary {
  cursor: pointer;
  padding: 5px 0;
  color: var(--muted);
}
.mongo-options-panel {
  position: absolute;
  top: calc(100% + 6px);
  right: 0;
  width: min(360px, calc(100vw - 48px));
  max-height: 50vh;
  overflow: auto;
  padding: 12px;
  border: 1px solid var(--raised);
  border-radius: 4px;
  background: var(--surface);
  box-shadow: 0 8px 24px rgb(0 0 0 / 20%);
}
.mongo-field {
  margin: 0;
}
.mongo-options-panel .mongo-field + .mongo-field {
  margin-top: 10px;
}
.mongo-error {
  color: var(--danger);
}
.mongo-documents {
  flex: 1;
  overflow: auto;
  min-height: 0;
  padding: 0 24px;
}
@media (max-width: 900px) {
  .mongo-pagination {
    flex-wrap: wrap;
  }
}
</style>
