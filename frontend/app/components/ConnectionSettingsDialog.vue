<script setup lang="ts">
import { ref, onMounted, useTemplateRef } from 'vue'
import type {
  ConnectionInput,
  Provider,
  SavedConnectionDetails,
  UpdateSavedConnection,
} from '../../shared/database'
import AppIcon from './AppIcon.vue'
import ConnectionForm from './ConnectionForm.vue'

const props = defineProps<{
  connectionId: string
  providers: Provider[]
  savingEnabled: boolean
  load: (id: string) => Promise<SavedConnectionDetails>
  update: (id: string, update: UpdateSavedConnection) => Promise<void>
  remove: (id: string) => Promise<boolean>
  test: (input: ConnectionInput) => Promise<{ ok: boolean; elapsedMs: number }>
  testSaved: (
    id: string,
    update: UpdateSavedConnection,
  ) => Promise<{ ok: boolean; elapsedMs: number }>
}>()
const emit = defineEmits<{ close: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const selected = ref<SavedConnectionDetails | null>(null)
const confirmingDelete = ref(false)
const busy = ref(false)
const error = ref('')
onMounted(() => {
  dialog.value?.showModal()
  void load()
})
async function load() {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    const connection = await props.load(props.connectionId)
    if (connection.id !== props.connectionId)
      throw new Error('The connection could not be loaded. Reopen its settings.')
    selected.value = connection
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : 'Could not load the connection.'
  } finally {
    busy.value = false
  }
}
async function save(update: UpdateSavedConnection) {
  if (!selected.value) return
  busy.value = true
  try {
    await props.update(props.connectionId, update)
  } finally {
    busy.value = false
  }
}
async function testChanges(update: UpdateSavedConnection) {
  if (!selected.value) throw new Error('The connection is not loaded.')
  busy.value = true
  try {
    return await props.testSaved(props.connectionId, update)
  } finally {
    busy.value = false
  }
}
async function remove() {
  if (!selected.value || !confirmingDelete.value || busy.value) return
  busy.value = true
  error.value = ''
  try {
    if (await props.remove(props.connectionId)) emit('close')
    else error.value = 'Could not delete the saved connection. Try again.'
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <dialog
    ref="dialog"
    class="connection-dialog"
    :aria-label="selected ? `Settings for ${selected.name}` : 'Connection settings'"
    @cancel="busy && $event.preventDefault()"
    @close="emit('close')"
  >
    <header class="settings-heading">
      <h2>{{ selected?.name ?? 'Connection settings' }}</h2>
      <button
        type="button"
        class="icon-button"
        aria-label="Close connection settings"
        :disabled="busy"
        @click="emit('close')"
      >
        <AppIcon name="close" />
      </button>
    </header>
    <p v-if="error" class="connection-feedback failure" role="alert">{{ error }}</p>
    <p v-if="busy && !selected" role="status">Loading…</p>
    <button
      v-if="error && !selected"
      type="button"
      class="button secondary small"
      :disabled="busy"
      @click="load"
    >
      Try again
    </button>
    <template v-if="selected">
      <ConnectionForm
        :key="connectionId"
        :initial="selected"
        :providers="providers"
        :busy="busy"
        error=""
        :saved-connections="[]"
        :saving-enabled="savingEnabled"
        :test-connection="test"
        :save-changes="save"
        :test-changes="testChanges"
        @saved="emit('close')"
      />
      <section class="delete-connection" aria-label="Delete this saved connection">
        <button
          v-if="!confirmingDelete"
          type="button"
          class="text-button"
          :disabled="busy"
          @click="confirmingDelete = true"
        >
          <AppIcon name="trash" :size="16" />Delete this saved connection
        </button>
        <template v-else>
          <p>Delete saved connection “{{ selected.name }}”? Your active session stays connected.</p>
          <div class="delete-actions">
            <button
              type="button"
              class="button secondary small"
              :disabled="busy"
              @click="confirmingDelete = false"
            >
              Cancel
            </button>
            <button type="button" class="button primary small" :disabled="busy" @click="remove">
              Delete saved connection
            </button>
          </div>
        </template>
      </section>
    </template>
  </dialog>
</template>

<style scoped>
.settings-heading,
.delete-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.settings-heading {
  margin-bottom: 20px;
}
.settings-heading h2 {
  margin: 0;
  font-size: 20px;
  overflow-wrap: anywhere;
}
.delete-connection {
  border-top: 1px solid var(--raised);
  padding-top: 20px;
  margin-top: 24px;
  font-size: 13px;
}
.delete-actions {
  justify-content: flex-end;
  flex-wrap: wrap;
  margin-top: 12px;
}
</style>
