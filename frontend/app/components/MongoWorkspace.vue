<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import type { TableInfo, TablePage } from '../../shared/database'
import { mongoFind } from '../utils/mongoQuery'

const props = defineProps<{
  table: TableInfo | null
  data: TablePage | null
  busy: boolean
  error: string
  page: number
  pageSize: number
  activeQuery: string | null
}>()
const emit = defineEmits<{
  find: [query: string]
  refresh: []
  page: [page: number]
  pageSize: [size: number]
  export: []
}>()
const filter = ref('{}')
const sort = ref('{}')
const projection = ref('{}')
const localError = ref('')
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
    localError.value = ''
  },
  { immediate: true },
)
function find() {
  if (props.busy) return
  localError.value = ''
  try {
    emit('find', mongoFind(props.table?.name ?? '', filter.value, sort.value, projection.value))
  } catch (cause) {
    localError.value = cause instanceof Error ? cause.message : 'Check the query.'
  }
}
function reset() {
  filter.value = sort.value = projection.value = '{}'
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
    <header class="mongo-heading">
      <div>
        <h1>{{ table?.name ?? 'Collections' }}</h1>
        <p>MongoDB · Experimental · Read only</p>
      </div>
      <button class="button secondary small" :disabled="busy || !data" @click="emit('export')">
        Export page as JSON
      </button>
    </header>
    <form class="mongo-query" @submit.prevent="find">
      <label
        >Filter<textarea
          v-model="filter"
          rows="3"
          spellcheck="false"
          placeholder='{ "status": "active" }'
          :disabled="busy || !table"
        />
      </label>
      <div class="mongo-options">
        <label
          >Sort<input
            v-model="sort"
            spellcheck="false"
            placeholder='{ "createdAt": -1 }'
            :disabled="busy || !table"
        /></label>
        <label
          >Projection<input
            v-model="projection"
            spellcheck="false"
            placeholder='{ "name": 1 }'
            :disabled="busy || !table"
        /></label>
        <button
          class="button secondary small"
          type="button"
          :disabled="busy || !table"
          @click="reset"
        >
          Reset
        </button>
        <button class="button primary small" :disabled="busy || !table">Find documents</button>
      </div>
      <p class="field-help">
        JSON objects only. Sort: 1 ascending, -1 descending. Projection: 1 include, 0 exclude. BSON
        values use Extended JSON, e.g. { "$oid": "507f1f77bcf86cd799439011" }.
      </p>
      <p v-if="localError || error" class="mongo-error" role="alert">{{ localError || error }}</p>
    </form>
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
      <details v-for="(row, index) in data?.rows ?? []" v-else :key="index" class="mongo-document">
        <summary>
          <span>{{ identity(row, index) }}</span><span>{{ Object.keys(row).length }} fields</span>
        </summary>
        <pre>{{ JSON.stringify(row, null, 2) }}</pre>
      </details>
    </section>
    <footer
      v-if="data"
      class="pagination table-footer mongo-pagination"
      aria-label="Document pagination"
    >
      <span>{{ data.rows.length }} documents · Page {{ page + 1 }}</span>
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
          class="button secondary small"
          :disabled="busy || page === 0"
          @click="emit('page', page - 1)"
        >
          Previous
        </button>
        <button
          class="button secondary small"
          :disabled="busy || !data.hasMore || (page + 1) * pageSize > 100000"
          @click="emit('page', page + 1)"
        >
          Next
        </button>
      </div>
    </footer>
  </main>
</template>
<style scoped>
.mongo-workspace {
  min-width: 0;
}
.mongo-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 20px 24px 12px;
  flex-wrap: wrap;
}
.mongo-heading h1 {
  font-size: 20px;
  margin: 0 0 4px;
  overflow-wrap: anywhere;
}
.mongo-heading p {
  color: var(--muted);
  margin: 0;
  font-size: 12px;
}
.mongo-query {
  padding: 8px 24px 16px;
  border-bottom: 1px solid var(--raised);
}
.mongo-query label {
  display: grid;
  gap: 6px;
  font-size: 12px;
  min-width: 0;
}
.mongo-query textarea,
.mongo-query input {
  width: 100%;
  min-width: 0;
  background: var(--canvas);
  color: var(--text);
  border: 1px solid var(--raised);
  border-radius: 4px;
  padding: 9px;
  font-family: 'IBM Plex Mono', monospace;
  font-size: 12px;
}
.mongo-query textarea {
  resize: vertical;
  min-height: 72px;
  max-height: 220px;
}
.mongo-query textarea:focus-visible,
.mongo-query input:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.mongo-options {
  display: grid;
  grid-template-columns: 1fr 1fr auto auto;
  align-items: end;
  gap: 10px;
  margin: 10px 0;
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
.mongo-document {
  border-bottom: 1px solid var(--raised);
}
.mongo-document summary {
  cursor: pointer;
  padding: 14px 0;
  font-size: 12px;
  overflow-wrap: anywhere;
}
.mongo-document summary span:first-child {
  font-family: 'IBM Plex Mono', monospace;
  margin-right: 16px;
}
.mongo-document summary span:last-child {
  color: var(--muted);
  white-space: nowrap;
}
.mongo-document pre {
  font:
    12px/1.7 'IBM Plex Mono',
    monospace;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  padding: 12px 16px;
  margin: 0 0 16px;
  background: var(--canvas);
  user-select: text;
}
@media (max-width: 900px) {
  .mongo-options {
    grid-template-columns: 1fr 1fr;
  }
  .mongo-pagination {
    flex-wrap: wrap;
  }
}
</style>
