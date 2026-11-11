<script setup lang="ts">
import { onMounted, useTemplateRef } from 'vue'
import { useToast } from '../composables/useToast'
import AppIcon from './AppIcon.vue'

import type { ColumnInfo } from '../../shared/database'
defineProps<{
  row: Record<string, unknown> | null
  columns: ColumnInfo[]
  rowNumber: number | null
  editable?: boolean
}>()
const emit = defineEmits<{ close: []; edit: []; remove: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
onMounted(() => dialog.value?.showModal())
const toast = useToast()
async function copy(row: Record<string, unknown>) {
  try {
    await navigator.clipboard.writeText(JSON.stringify(row, null, 2))
    toast.show('Record copied to clipboard.')
  } catch {
    toast.show('Clipboard unavailable in this browser.', 'error')
  }
}
function value(input: unknown) {
  return input === null
    ? 'NULL'
    : typeof input === 'object'
      ? JSON.stringify(input, null, 2)
      : String(input)
}
</script>
<template>
  <dialog
    ref="dialog"
    class="record-dialog"
    aria-label="Record details"
    @close="emit('close')"
    @click="$event.target === dialog && dialog?.close()"
  >
    <div class="inspector">
      <div class="inspector-heading">
        <span>Record details</span
        ><button
          class="icon-button"
          title="Close details"
          aria-label="Close details"
          autofocus
          @click="dialog?.close()"
        >
          <AppIcon name="close" :size="16" />
        </button>
      </div>
      <template v-if="row"
        ><div class="record-meta">
          <span class="tiny-dot" />Row {{ rowNumber
          }}<span>{{ editable ? 'Record' : 'Read only' }}</span>
        </div>
        <dl class="record-fields">
          <div v-for="column in columns" :key="column.name">
            <dt>{{ column.name }}<AppIcon v-if="column.primaryKey" name="key" :size="12" /></dt>
            <dd :class="{ 'null-cell': row[column.name] === null }">
              {{ value(row[column.name]) }}
            </dd>
            <span class="field-type">{{ column.dataType }}</span>
          </div>
        </dl>
        <div class="inspector-footer">
          <button v-if="editable" class="button" @click="emit('edit')">Edit record</button>
          <button v-if="editable" class="button secondary" @click="emit('remove')">
            Delete record
          </button>
          <button class="button secondary" @click="copy(row)">
            <AppIcon name="download" :size="15" />Copy record as JSON
          </button>
        </div></template
      >
      <div v-else class="inspector-empty">
        <AppIcon name="panel" :size="28" />
        <h3>A closer look</h3>
        <p>Double-click a row to explore<br />its fields and values.</p>
        <kbd>Enter</kbd><span> to inspect a focused row</span>
      </div>
    </div>
  </dialog>
</template>
