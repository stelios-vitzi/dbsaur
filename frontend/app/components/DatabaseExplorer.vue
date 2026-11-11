<script setup lang="ts">
import { ref, computed, watch, useId } from 'vue'
import AppIcon from './AppIcon.vue'

import type { TableInfo } from '../../shared/database'

const props = defineProps<{
  tables: TableInfo[]
  activeDatabase: string
  selectedTable: TableInfo | null
  loading: boolean
  disabled: boolean
  canCreateTable: boolean
}>()
const emit = defineEmits<{
  choose: [database: string, table: TableInfo]
  refresh: []
  createTable: []
  context: [event: MouseEvent, table: TableInfo]
  schemaContext: [event: MouseEvent, schema: string]
}>()
const filter = ref('')
const expanded = ref(new Set(['public', 'dbo']))
watch(
  () => props.tables,
  (tables, previous) => {
    if (!previous?.length && tables[0]) expanded.value.add(tables[0].schema)
  },
  { immediate: true },
)
const treeId = useId()
const filteredTables = computed(() => {
  const query = filter.value.trim().toLowerCase()
  return props.tables.filter((table) =>
    `${table.schema}.${table.name}`.toLowerCase().includes(query),
  )
})
const schemas = computed(() => {
  const names = [...new Set(filteredTables.value.map((table) => table.schema))]
  return [
    ...names.filter((name) => name === 'public'),
    ...names.filter((name) => name !== 'public'),
  ]
})
function isExpanded(schema: string) {
  return !!filter.value.trim() || expanded.value.has(schema)
}
function toggle(schema: string) {
  if (expanded.value.has(schema)) expanded.value.delete(schema)
  else expanded.value.add(schema)
}
watch(
  () => props.activeDatabase,
  () => {
    expanded.value = new Set(['public', 'dbo'])
    filter.value = ''
  },
)
</script>
<template>
  <div class="explorer-toolbar">
    <label class="search-box table-search">
      <AppIcon name="search" :size="15" />
      <input v-model="filter" placeholder="Find a table…" aria-label="Find a table" />
    </label>
    <button
      v-if="canCreateTable"
      class="icon-button explorer-create-table"
      title="Create table"
      aria-label="Create table"
      :disabled="disabled || loading"
      @click="emit('createTable')"
    >
      <AppIcon name="plus" :size="16" />
    </button>
    <button
      class="icon-button explorer-refresh"
      title="Refresh explorer"
      aria-label="Refresh explorer"
      :disabled="disabled || loading"
      @click="emit('refresh')"
    >
      <AppIcon name="refresh" :size="14" />
    </button>
  </div>
  <nav class="table-list database-tree" aria-label="Tables" :aria-busy="loading">
    <p v-if="loading" class="sidebar-empty" role="status">Loading tables…</p>
    <template v-else>
      <section v-for="(schema, index) in schemas" :key="schema" class="schema-group">
        <button
          class="schema-label"
          :aria-expanded="isExpanded(schema)"
          :aria-controls="`${treeId}-${index}`"
          :disabled="disabled || !!filter.trim()"
          @click="toggle(schema)"
          @contextmenu.prevent.stop="emit('schemaContext', $event, schema)"
        >
          <AppIcon :name="isExpanded(schema) ? 'down' : 'chevron'" :size="12" />
          <span class="schema-name">{{ schema }}</span>
          <span>{{ filteredTables.filter((table) => table.schema === schema).length }}</span>
        </button>
        <div v-show="isExpanded(schema)" :id="`${treeId}-${index}`">
          <button
            v-for="table in filteredTables.filter((table) => table.schema === schema)"
            :key="`${table.schema}.${table.name}`"
            class="table-link"
            :disabled="disabled"
            :class="{
              active: table.schema === selectedTable?.schema && table.name === selectedTable?.name,
            }"
            :aria-current="
              table.schema === selectedTable?.schema && table.name === selectedTable?.name
                ? 'true'
                : undefined
            "
            :title="`${activeDatabase}.${table.schema}.${table.name}`"
            @click="emit('choose', activeDatabase, table)"
            @contextmenu.prevent.stop="emit('context', $event, table)"
          >
            <AppIcon name="table" :size="13" /><span>{{ table.name }}</span>
          </button>
        </div>
      </section>
      <p v-if="!filteredTables.length" class="sidebar-empty">
        {{ filter.trim() ? 'No matching tables.' : 'No accessible tables found.' }}
      </p>
    </template>
  </nav>
</template>
