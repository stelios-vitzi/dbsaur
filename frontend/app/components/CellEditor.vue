<script setup lang="ts">
import { ref, computed, watch, onMounted, onScopeDispose, nextTick, useTemplateRef } from 'vue'
import { useActionFeedback } from '../composables/useToast'
import AppIcon from './AppIcon.vue'
import GeometryMap from './GeometryMap.vue'
import DateTimePicker from './DateTimePicker.vue'
import { temporalType } from '../utils/temporal'

import { DatabaseChangeCancelled } from '../utils/databaseChange'
import type { ColumnInfo, Mutation, TableInfo } from '../../shared/database'
import { isSpatialType } from '../utils/geometry'
const props = defineProps<{
  column: ColumnInfo
  provider?: string
  row: Record<string, unknown>
  table: TableInfo | null
  editable: boolean
  readonlyReason?: string
  apply: (mutation: Mutation) => Promise<{ affectedRows: number }>
}>()
const emit = defineEmits<{ close: []; saved: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const input = useTemplateRef<HTMLTextAreaElement>('input')
const temporalPicker = useTemplateRef<InstanceType<typeof DateTimePicker>>('temporalPicker')
const temporal = computed(() => temporalType(props.column.dataType, props.provider))
const rawEditing = ref(false)
const dateValid = ref(true)
const original = props.row[props.column.name]
const value = ref(
  original === null
    ? ''
    : typeof original === 'object'
      ? JSON.stringify(original)
      : String(original),
)
const mode = ref(original === null ? 'null' : 'value')
const saving = ref(false)
const mapOpen = ref(false)
const error = ref('')
useActionFeedback(error)
const contentWidth = ref(340)
let measurement: CanvasRenderingContext2D | null = null
function fitHeight() {
  const field = input.value
  if (!field) return
  const scrollTop = field.scrollTop
  field.style.height = '0px'
  field.style.height = `${field.scrollHeight + 2}px`
  field.scrollTop = scrollTop
}
async function fitContent() {
  if (temporal.value && canEdit.value && !rawEditing.value) {
    contentWidth.value = 420
    return
  }
  if (!measurement || !input.value) return
  measurement.font = getComputedStyle(input.value).font
  let width = 0
  for (const line of (mode.value === 'null' ? 'NULL' : value.value).split('\n')) {
    width = Math.max(width, measurement.measureText(line).width)
    if (width >= 620) break
  }
  contentWidth.value = Math.ceil(Math.min(680, Math.max(340, width + 60)))
  await nextTick()
  fitHeight()
}
watch([value, mode], fitContent, { flush: 'post' })
let resizeObserver: ResizeObserver | undefined
const canEdit = computed(() => props.editable && !props.column.readOnly)
const changed = computed(() =>
  mode.value === 'null'
    ? original !== null
    : original === null ||
      value.value !== (typeof original === 'object' ? JSON.stringify(original) : String(original)),
)
async function focusValue() {
  await nextTick()
  if (temporal.value && canEdit.value && !rawEditing.value) {
    temporalPicker.value?.focus()
    return
  }
  input.value?.focus()
  input.value?.select()
}
function editValue(event: Event) {
  value.value = (event.target as HTMLTextAreaElement).value
  mode.value = 'value'
}
function editTemporalValue(next: string) {
  value.value = next
  mode.value = 'value'
}
async function toggleTextEditing() {
  rawEditing.value = !rawEditing.value
  await fitContent()
  await focusValue()
}
onMounted(async () => {
  dialog.value?.showModal()
  measurement = document.createElement('canvas').getContext('2d')
  await fitContent()
  let previousWidth = input.value?.clientWidth
  resizeObserver = new ResizeObserver(() => {
    const width = input.value?.clientWidth
    if (width !== previousWidth) {
      previousWidth = width
      fitHeight()
    }
  })
  if (input.value) resizeObserver.observe(input.value)
  await focusValue()
})
onScopeDispose(() => resizeObserver?.disconnect())
async function save() {
  if (
    saving.value ||
    !canEdit.value ||
    !changed.value ||
    !props.table ||
    (temporal.value && mode.value !== 'null' && !rawEditing.value && !dateValid.value)
  )
    return
  saving.value = true
  error.value = ''
  try {
    await props.apply({
      action: 'update',
      schema: props.table.schema,
      table: props.table.name,
      original: props.row,
      values: { [props.column.name]: mode.value === 'null' ? null : value.value },
    })
    emit('saved')
  } catch (cause) {
    if (cause instanceof DatabaseChangeCancelled) {
      emit('close')
      return
    }
    error.value = cause instanceof Error ? cause.message : 'Could not save this value. Try again.'
  } finally {
    saving.value = false
  }
}
</script>
<template>
  <dialog
    ref="dialog"
    class="cell-editor"
    :class="{ 'temporal-cell-editor': temporal && canEdit }"
    :style="{ '--cell-editor-width': `${contentWidth}px` }"
    :aria-label="`${canEdit ? 'Edit' : 'View'} ${column.name}`"
    @close="emit('close')"
    @cancel="saving && $event.preventDefault()"
    @click="$event.target === dialog && !saving && emit('close')"
  >
    <form
      @submit.prevent="save"
      @keydown.ctrl.enter.prevent="save"
      @keydown.meta.enter.prevent="save"
    >
      <div class="cell-editor-heading">
        <label for="cell-value">{{ column.name }}</label><span class="muted">{{
          column.dataType
        }}</span
        ><button
          v-if="isSpatialType(column.dataType) && typeof original === 'string'"
          type="button"
          class="icon-button cell-map-button"
          title="View on map"
          aria-label="View on map"
          @click="mapOpen = true"
        >
          <AppIcon name="map" :size="18" /></button
        ><button
          type="button"
          class="icon-button"
          aria-label="Close value editor"
          :disabled="saving"
          @click="emit('close')"
        >
          <AppIcon name="close" :size="16" />
        </button>
      </div>
      <div class="cell-editor-controls">
        <DateTimePicker
          v-if="temporal && canEdit && !rawEditing"
          ref="temporalPicker"
          :model-value="value"
          :type="temporal"
          :disabled="saving"
          @update:model-value="editTemporalValue"
          @validity="dateValid = $event"
        />
        <textarea
          v-if="!temporal || !canEdit || rawEditing"
          id="cell-value"
          ref="input"
          :value="mode === 'null' ? '' : value"
          rows="1"
          :readonly="!canEdit"
          :disabled="saving"
          :placeholder="mode === 'null' ? 'NULL' : 'Value'"
          spellcheck="false"
          @input="editValue"
        />
        <button
          v-if="temporal && canEdit"
          type="button"
          class="text-button temporal-text-toggle"
          :disabled="saving"
          @click="toggleTextEditing"
        >
          {{ rawEditing ? 'Use date and time picker' : 'Edit as text' }}
        </button>
        <div class="cell-editor-actions">
          <select
            v-if="canEdit && column.nullable"
            v-model="mode"
            aria-label="Value or NULL"
            :disabled="saving"
            @change="focusValue"
          >
            <option value="value">Value</option>
            <option value="null">NULL</option>
          </select>
          <button
            v-if="canEdit"
            type="submit"
            class="button small"
            :disabled="
              saving || !changed || (!!temporal && mode !== 'null' && !rawEditing && !dateValid)
            "
          >
            {{ saving ? 'Saving…' : 'Save value' }}
          </button>
          <button
            type="button"
            class="button secondary small"
            :disabled="saving"
            @click="emit('close')"
          >
            {{ canEdit ? 'Cancel' : 'Close' }}
          </button>
        </div>
      </div>
      <p v-if="!canEdit" class="muted">
        {{
          readonlyReason ||
          (column.readOnly && editable ? 'Generated by the database.' : 'This value is read only.')
        }}
      </p>
    </form>
    <GeometryMap
      v-if="mapOpen"
      :value="String(original)"
      :column="column.name"
      @close="mapOpen = false"
    />
  </dialog>
</template>

<style scoped>
.temporal-cell-editor .cell-editor-controls {
  flex-direction: column;
  align-items: stretch;
}
.temporal-cell-editor .cell-editor-actions {
  flex-direction: row;
  align-items: center;
  justify-content: flex-end;
}
.temporal-cell-editor .temporal-text-toggle {
  align-self: flex-start;
  color: var(--muted);
  font-size: var(--text-sm);
}
.cell-editor-heading .cell-map-button {
  margin-left: auto;
  color: var(--accent);
}
.cell-editor-heading .cell-map-button + .icon-button {
  margin-left: 0;
}
</style>
