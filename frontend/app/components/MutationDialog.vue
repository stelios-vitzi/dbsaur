<script setup lang="ts">
import { ref, computed, watch, onMounted, useTemplateRef } from 'vue'
import { useActionFeedback } from '../composables/useToast'
import AppIcon from './AppIcon.vue'
import { DatabaseChangeCancelled } from '../utils/databaseChange'
import { providerOptions, quoteIdentifier } from '../providers'
import type { Mutation, TableInfo } from '../../shared/database'

const props = defineProps<{
  provider: string
  mode: 'delete' | 'createDatabase'
  table: TableInfo | null
  row: Record<string, unknown> | null
  database: string
  apply: (mutation: Mutation) => Promise<{ affectedRows: number }>
}>()
const emit = defineEmits<{ close: []; saved: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
onMounted(() => dialog.value?.showModal())
const saving = ref(false)
const error = ref('')
useActionFeedback(error)
const confirmation = ref('')
const name = ref('')
const options = computed(() => providerOptions(props.provider))
const title = computed(() =>
  props.mode === 'createDatabase' ? 'Create database' : 'Delete record',
)
const target = computed(() =>
  props.table
    ? [props.table.schema, props.table.name]
        .map((value) => quoteIdentifier(value, props.provider))
        .join('.')
    : '',
)
watch(name, () => {
  confirmation.value = ''
  error.value = ''
})
const canApply = computed(
  () =>
    confirmation.value === 'APPLY' &&
    !saving.value &&
    (props.mode === 'createDatabase' ? !!name.value.trim() : !!props.table && !!props.row),
)
async function save() {
  if (!canApply.value) return
  const mutation: Mutation =
    props.mode === 'createDatabase'
      ? { action: 'createDatabase', name: name.value }
      : {
          action: 'delete',
          schema: props.table!.schema,
          table: props.table!.name,
          original: props.row!,
        }
  saving.value = true
  error.value = ''
  try {
    await props.apply(mutation)
    emit('saved')
  } catch (cause) {
    if (cause instanceof DatabaseChangeCancelled) {
      emit('close')
      return
    }
    error.value =
      cause instanceof Error ? cause.message : 'The change failed. Review the values and try again.'
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
      <div class="inspector-heading">
        <h2>{{ title }}</h2>
        <button
          type="button"
          class="icon-button"
          aria-label="Close editor"
          :disabled="saving"
          @click="emit('close')"
        >
          <AppIcon name="close" :size="18" />
        </button>
      </div>
      <p class="muted">
        {{ database }}<template v-if="mode === 'delete'"> / {{ target }}</template>
      </p>
      <label v-if="mode === 'createDatabase'"
        >Database name
        <input
          v-model="name"
          :disabled="saving"
          required
          :maxlength="options.nameLimit"
          autocomplete="off"
        />
      </label>
      <template v-else>
        <p class="mutation-warning">
          Permanently delete this record. Foreign key rules may block the deletion or cascade it to
          dependent records.
        </p>
        <pre class="mutation-record">{{ JSON.stringify(row, null, 2) }}</pre>
        <p v-if="provider !== 'postgres'" class="muted">
          Deleting requires a primary key and an unchanged record.
        </p>
      </template>
      <label
        >Type APPLY to confirm this change
        <input v-model="confirmation" :disabled="saving" autocomplete="off" spellcheck="false" />
      </label>
      <div class="mutation-actions">
        <button type="button" class="button secondary" :disabled="saving" @click="emit('close')">
          Cancel
        </button>
        <button type="submit" class="button" :disabled="!canApply">
          {{ saving ? 'Applying…' : mode === 'delete' ? 'Delete record' : 'Create database' }}
        </button>
      </div>
    </form>
  </dialog>
</template>
