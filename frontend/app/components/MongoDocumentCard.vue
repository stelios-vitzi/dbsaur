<script setup lang="ts">
import { ref } from 'vue'
import JsonView from './JsonView.vue'
import JsonTree from './JsonTree.vue'
import AppIcon from './AppIcon.vue'
import ContextMenu from './ContextMenu.vue'
const props = defineProps<{
  row: Record<string, unknown>
  identity: string
  editable: boolean
  disabled: boolean
  editDisabledReason: string
}>()
const emit = defineEmits<{ edit: []; delete: [] }>()
const open = ref(false)
const tree = ref(true)
const copyFeedback = ref('')
async function copyDocument() {
  try {
    await navigator.clipboard.writeText(JSON.stringify(props.row, null, 2))
    copyFeedback.value = 'Document copied.'
  } catch {
    copyFeedback.value = 'Could not copy the document. Check clipboard permissions.'
  }
}
const menu = ref<{ x: number; y: number } | null>(null)
function showActions(event: Event) {
  const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect()
  menu.value = { x: bounds.left, y: bounds.bottom + 2 }
}
</script>
<template>
  <details class="mongo-document" @toggle="open = ($event.target as HTMLDetailsElement).open">
    <summary>
      <span>{{ identity }}</span><span>{{ Object.keys(row).length }} fields</span>
      <span class="document-actions">
        <button
          type="button"
          class="icon-button"
          aria-label="Document actions"
          aria-haspopup="menu"
          :aria-expanded="!!menu"
          :disabled="disabled"
          title="Document actions"
          @click.stop.prevent="showActions"
          @keydown.down.stop.prevent="showActions"
        >
          <AppIcon name="down" :size="16" />
        </button>
      </span>
    </summary>
    <template v-if="open">
      <div class="document-view-toolbar">
        <button type="button" class="text-button" :aria-pressed="tree" @click="tree = true">
          Tree
        </button>
        <button type="button" class="text-button" :aria-pressed="!tree" @click="tree = false">
          JSON
        </button>
      </div>
      <div v-if="tree" class="document-tree"><JsonTree :value="row" /></div>
      <JsonView v-else :value="row" />
      <p v-if="editDisabledReason" class="field-help">{{ editDisabledReason }}</p>
    </template>
    <p v-if="copyFeedback" class="field-help" role="status">{{ copyFeedback }}</p>
    <ContextMenu
      v-if="menu"
      v-bind="menu"
      label="Document actions"
      compact
      :actions="[
        { label: 'Edit document…', disabled: disabled || !editable, run: () => emit('edit') },
        { label: 'Delete document…', disabled: disabled || !editable, run: () => emit('delete') },
        { label: 'Copy document as JSON', disabled: disabled, run: copyDocument },
      ]"
      @close="menu = null"
    />
  </details>
</template>
<style scoped>
.mongo-document {
  border-bottom: 1px solid var(--raised);
}
summary {
  cursor: pointer;
  padding: 8px 0;
  font-size: 12px;
  overflow-wrap: anywhere;
}
summary > span:first-child {
  font-family: var(--font-code);
  margin-right: 16px;
}
summary > span:nth-child(2) {
  color: var(--muted);
  white-space: nowrap;
}
.document-actions {
  display: inline-flex;
  float: right;
  margin-left: 16px;
  gap: 4px;
}
.field-help {
  margin: 0 0 8px;
}
.mongo-document :deep(.json-view) {
  margin: 0 0 8px;
  padding: 8px 12px;
}
.document-view-toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  padding-bottom: 4px;
  font-size: 12px;
  color: var(--muted);
}
.document-view-toolbar [aria-pressed='true'] {
  color: var(--accent);
  font-weight: 600;
}
.document-tree {
  padding: 8px 12px;
  margin-bottom: 8px;
  background: var(--canvas);
  user-select: text;
}
.document-tree :deep(.json-leaf .json-view) {
  margin: 0;
  padding: 0;
}
</style>
