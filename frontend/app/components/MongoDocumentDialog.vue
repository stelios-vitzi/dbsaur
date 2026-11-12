<script setup lang="ts">
import { ref, computed, watch, onMounted, useTemplateRef } from 'vue'
import type { Mutation, TableInfo } from '../../shared/database'
import { mongoObject } from '../utils/mongoQuery'
import { DatabaseChangeCancelled } from '../utils/databaseChange'
import JsonEditor from './JsonEditor.vue'
import JsonView from './JsonView.vue'

const props = defineProps<{
  table: TableInfo
  mode: 'insert' | 'replace' | 'delete'
  row?: Record<string, unknown>
  snapshot?: string
  apply: (mutation: Mutation) => Promise<{ affectedRows: number }>
}>()
const emit = defineEmits<{ close: []; saved: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
onMounted(() => dialog.value?.showModal())
const document = ref(JSON.stringify(props.row ?? {}, null, 2))
const confirmation = ref('')
const saving = ref(false)
const error = ref('')
const title = computed(
  () =>
    ({ insert: 'Insert document', replace: 'Replace document', delete: 'Delete document' })[
      props.mode
    ],
)
watch(document, () => {
  confirmation.value = ''
  error.value = ''
})
async function save() {
  if (saving.value || confirmation.value !== 'APPLY') return
  error.value = ''
  try {
    const target = { schema: props.table.schema, table: props.table.name }
    let mutation: Mutation
    if (props.mode === 'insert')
      mutation = {
        action: 'insertDocument',
        ...target,
        document: mongoObject(document.value, 'Document'),
      }
    else {
      if (!props.snapshot) throw new Error('Refresh the complete document before editing.')
      mutation =
        props.mode === 'delete'
          ? { action: 'deleteDocument', ...target, snapshot: props.snapshot }
          : {
              action: 'replaceDocument',
              ...target,
              snapshot: props.snapshot,
              document: mongoObject(document.value, 'Document'),
            }
    }
    saving.value = true
    await props.apply(mutation)
    emit('saved')
  } catch (cause) {
    if (cause instanceof DatabaseChangeCancelled) {
      emit('close')
      return
    }
    error.value = cause instanceof Error ? cause.message : 'The document change failed.'
    confirmation.value = ''
  } finally {
    saving.value = false
  }
}
</script>
<template>
  <dialog
    ref="dialog"
    class="mutation-dialog"
    :aria-label="title"
    @cancel="saving && $event.preventDefault()"
    @close="emit('close')"
  >
    <form @submit.prevent="save">
      <h2>{{ title }}</h2>
      <p class="muted">{{ table.schema }} / {{ table.name }}</p>
      <p class="field-help">
        {{
          mode === 'insert'
            ? 'Enter a JSON document. Omit _id to let MongoDB generate one.'
            : mode === 'replace'
              ? 'Save the complete document, including _id. Removing a field here removes it from the database.'
              : 'Permanently delete this document.'
        }}
        BSON types use Extended JSON. Editing is limited to 1 MiB.
      </p>
      <JsonView v-if="mode === 'delete'" :value="row" />
      <JsonEditor
        v-else
        v-model="document"
        label="Document Extended JSON"
        :height="300"
        :disabled="saving"
      />
      <p v-if="mode !== 'insert'" class="field-help">
        If the document has changed since loading, refresh and review it before trying again.
      </p>
      <p v-if="error" role="alert" class="document-error">{{ error }}</p>
      <label
        >Type APPLY to confirm this change<input
          v-model="confirmation"
          :disabled="saving"
          autocomplete="off"
          spellcheck="false"
      /></label>
      <div class="mutation-actions">
        <button type="button" class="button secondary" :disabled="saving" @click="emit('close')">
          Cancel
        </button>
        <button class="button primary" :disabled="saving || confirmation !== 'APPLY'">
          {{ saving ? 'Applying…' : title }}
        </button>
      </div>
    </form>
  </dialog>
</template>
<style scoped>
.document-error {
  color: var(--danger);
}
</style>
