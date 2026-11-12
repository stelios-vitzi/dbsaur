<script setup lang="ts">
import { onMounted, useTemplateRef } from 'vue'
import AppIcon from './AppIcon.vue'

defineEmits<{ close: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
onMounted(() => dialog.value?.showModal())
const shortcuts = [
  ['Ctrl / ⌘ + Enter', 'Run query'],
  ['Ctrl + Space', 'Show SQL suggestions'],
  ['↑ / ↓', 'Choose a suggestion'],
  ['Tab / Enter', 'Accept a suggestion'],
  ['Esc', 'Dismiss suggestions or close a dialog'],
  ['Ctrl / ⌘ + Z', 'Undo in the SQL editor'],
  ['Ctrl / ⌘ + Shift + Z', 'Redo in the SQL editor'],
  ['Tab', 'Indent when suggestions are closed'],
  ['Shift + Tab', 'Reduce indentation'],
]
</script>

<template>
  <dialog
    ref="dialog"
    class="shortcuts-dialog"
    aria-labelledby="shortcuts-title"
    @close="$emit('close')"
  >
    <header class="inspector-heading">
      <h2 id="shortcuts-title">Keyboard shortcuts</h2>
      <button class="icon-button" aria-label="Close keyboard shortcuts" @click="dialog?.close()">
        <AppIcon name="close" :size="18" />
      </button>
    </header>
    <p class="muted">Query editor · Use ⌘ on macOS where shown.</p>
    <dl>
      <div v-for="[keys, description] in shortcuts" :key="keys">
        <dt>{{ description }}</dt>
        <dd>
          <kbd>{{ keys }}</kbd>
        </dd>
      </div>
    </dl>
    <form method="dialog"><button class="button secondary small">Close</button></form>
  </dialog>
</template>

<style scoped>
.shortcuts-dialog {
  width: min(560px, calc(100vw - 32px));
  max-height: calc(100dvh - 32px);
  overflow: auto;
  padding: 20px;
  color: var(--text);
  background: var(--sidebar);
  border: 1px solid var(--border);
  border-radius: 8px;
}
.shortcuts-dialog::backdrop {
  background: #0008;
}
.inspector-heading {
  padding: 0 0 12px;
}
h2 {
  font-size: var(--text-body);
}
p,
dl {
  font-size: var(--text-sm);
}
dl {
  margin-block: 16px;
}
dl > div {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  padding-block: 10px;
  border-bottom: 1px solid var(--border);
}
dd {
  margin: 0;
  text-align: right;
}
kbd {
  font-family: var(--font-code);
  color: var(--secondary);
}
form {
  display: flex;
  justify-content: flex-end;
}
</style>
