<script setup lang="ts">
import { ref, onMounted, onScopeDispose, useTemplateRef } from 'vue'
import AppIcon from './AppIcon.vue'
import { $fetch } from '../services/http'
import type { ColumnInfo, TableInfo, TablePage } from '../../shared/database'

const props = defineProps<{ table: TableInfo }>()
const emit = defineEmits<{ close: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const columns = ref<ColumnInfo[]>([])
const loading = ref(true)
const error = ref('')
const controller = new AbortController()
async function load() {
  loading.value = true
  error.value = ''
  try {
    const page = await $fetch<TablePage>('/api/rows', {
      query: { schema: props.table.schema, table: props.table.name, page: 0, pageSize: 1 },
      signal: controller.signal,
    })
    columns.value = page.columns
  } catch {
    if (!controller.signal.aborted) error.value = 'Could not load the table structure. Try again.'
  } finally {
    loading.value = false
  }
}
onMounted(() => {
  dialog.value?.showModal()
  void load()
})
onScopeDispose(() => controller.abort())
</script>

<template>
  <dialog
    ref="dialog"
    class="table-structure-dialog"
    aria-labelledby="structure-title"
    @close="emit('close')"
  >
    <header class="inspector-heading">
      <div>
        <h2 id="structure-title">Table structure</h2>
        <p class="muted">{{ table.schema }}.{{ table.name }}</p>
      </div>
      <button class="icon-button" aria-label="Close table structure" @click="dialog?.close()">
        <AppIcon name="close" :size="18" />
      </button>
    </header>
    <p v-if="loading" class="structure-status" role="status">Loading structure…</p>
    <div v-else-if="error" class="structure-status" role="alert">
      <p>{{ error }}</p>
      <button class="button secondary small" @click="load">Try again</button>
    </div>
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
          <tr v-for="column in columns" :key="column.name">
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
    </div>
  </dialog>
</template>

<style scoped>
.table-structure-dialog {
  width: min(760px, calc(100vw - 28px));
  max-height: calc(100dvh - 48px);
  padding: 0;
  overflow: hidden;
  color: var(--text);
  background: var(--sidebar);
  border: 1px solid var(--raised);
  border-radius: 8px;
  font-size: var(--text-sm);
}
.table-structure-dialog::backdrop {
  background: #0009;
}
.inspector-heading {
  height: auto;
  padding: 16px 20px;
  border-bottom: 1px solid var(--raised);
}
h2 {
  font-size: var(--text-body);
}
.grid-scroll {
  max-height: calc(100dvh - 160px);
}
.structure-status {
  padding: 20px;
}
.structure-status button {
  margin-top: 12px;
}
.structure-grid {
  font-size: inherit;
}
</style>
