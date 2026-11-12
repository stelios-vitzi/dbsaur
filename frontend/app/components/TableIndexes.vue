<script setup lang="ts">
import { ref, computed, onMounted, onScopeDispose } from 'vue'
import AppIcon from './AppIcon.vue'
import { $fetch } from '../services/http'
import { providerOptions } from '../providers'
import { DatabaseChangeCancelled } from '../utils/databaseChange'
import type { IndexInfo, Mutation, TableInfo } from '../../shared/database'

const props = defineProps<{
  table: TableInfo
  provider: string
  disabled: boolean
  apply: (mutation: Mutation) => Promise<{ affectedRows: number }>
}>()
const emit = defineEmits<{ saved: []; busy: [value: boolean] }>()
const indexes = ref<IndexInfo[]>([])
const loading = ref(true)
const saving = ref(false)
const error = ref('')
const draft = ref<IndexInfo>()
const original = ref<IndexInfo>()
const deleting = ref(false)
const controller = new AbortController()
onScopeDispose(() => controller.abort())
async function load() {
  loading.value = true
  error.value = ''
  try {
    indexes.value = await $fetch<IndexInfo[]>('/api/indexes', {
      query: { schema: props.table.schema, table: props.table.name },
      signal: controller.signal,
    })
  } catch {
    if (!controller.signal.aborted) error.value = 'Could not load indexes. Try again.'
  } finally {
    loading.value = false
  }
}
onMounted(load)
function edit(index?: IndexInfo, remove = false) {
  original.value = index
  draft.value = index
    ? { ...index, columns: [...index.columns] }
    : { name: '', columns: [], unique: false, editable: true }
  deleting.value = remove
  error.value = ''
}
const valid = computed(() => {
  const index = draft.value
  if (!index || saving.value || props.disabled) return false
  if (deleting.value) return true
  const name = index.name.trim()
  return (
    !!name &&
    !/[\u0000-\u001f\u007f]/.test(name) &&
    new TextEncoder().encode(name).length <= providerOptions(props.provider).nameLimit &&
    index.columns.length > 0 &&
    index.columns.length <= 32 &&
    new Set(index.columns).size === index.columns.length &&
    index.columns.every((column) => props.table.columns.includes(column)) &&
    !indexes.value.some(
      (item) =>
        item !== original.value &&
        (props.provider === 'postgres'
          ? item.name === name
          : item.name.toLowerCase() === name.toLowerCase()),
    ) &&
    JSON.stringify({ ...index, name }) !== JSON.stringify(original.value)
  )
})
async function save() {
  if (!valid.value || !draft.value) return
  saving.value = true
  emit('busy', true)
  error.value = ''
  try {
    await props.apply({
      action: 'alterIndex',
      schema: props.table.schema,
      table: props.table.name,
      expected: original.value ?? null,
      index: deleting.value ? null : { ...draft.value, name: draft.value.name.trim() },
    })
    draft.value = undefined
    emit('saved')
  } catch (cause) {
    if (!(cause instanceof DatabaseChangeCancelled))
      error.value = cause instanceof Error ? cause.message : 'Could not update the index.'
  } finally {
    saving.value = false
    emit('busy', false)
  }
}
</script>

<template>
  <section class="schema-new-fields" aria-label="Table indexes">
    <div class="schema-section-heading">
      <h3>
        Indexes <span>{{ indexes.length }}</span>
      </h3>
      <button
        type="button"
        class="button secondary small"
        :disabled="disabled || saving || loading || !!draft"
        @click="edit()"
      >
        <AppIcon name="plus" :size="14" />Add index
      </button>
    </div>
    <p class="schema-section-note">
      Column order determines how an index is used. Manage key constraints with the primary and
      unique key controls above.
    </p>
    <p v-if="loading" class="schema-empty" role="status">Loading indexes…</p>
    <p v-else-if="!indexes.length && !error" class="schema-empty">No indexes defined.</p>
    <div v-for="index in indexes" :key="index.name" class="schema-unique-key index-row">
      <div>
        <strong>{{ index.name }}</strong><span v-if="index.unique" class="muted"> · Unique</span>
        <p class="schema-section-note">{{ index.columns.join(', ') }}</p>
        <p v-if="!index.editable" class="schema-section-note">
          Constraint-backed or advanced index. Manage through keys or SQL.
        </p>
      </div>
      <div class="index-actions">
        <button
          type="button"
          class="icon-button"
          :aria-label="`Edit index ${index.name}`"
          title="Edit index"
          :disabled="disabled || saving || !!draft || !index.editable"
          @click="edit(index)"
        >
          <AppIcon name="edit" :size="15" />
        </button>
        <button
          type="button"
          class="icon-button"
          :aria-label="`Delete index ${index.name}`"
          title="Delete index"
          :disabled="disabled || saving || !!draft || !index.editable"
          @click="edit(index, true)"
        >
          <AppIcon name="minus" :size="15" />
        </button>
      </div>
    </div>
    <div v-if="draft" class="schema-unique-key index-draft">
      <h3>{{ deleting ? 'Delete index' : original ? 'Edit index' : 'Add index' }}</h3>
      <p v-if="deleting" class="schema-section-note">
        Delete {{ draft.name }}? Queries that use this index may become slower.
      </p>
      <template v-else>
        <label
          >Index name<input
            v-model="draft.name"
            :disabled="saving"
            :maxlength="providerOptions(provider).nameLimit"
            autocomplete="off"
            spellcheck="false"
        /></label>
        <label class="schema-field-check"
          ><input v-model="draft.unique" type="checkbox" :disabled="saving" />Unique</label
        >
        <div v-for="(_, position) in draft.columns" :key="position" class="index-actions">
          <label
            >Column {{ position + 1
            }}<select v-model="draft.columns[position]" :disabled="saving">
              <option value="" disabled>Select column</option>
              <option
                v-for="column in table.columns"
                :key="column"
                :value="column"
                :disabled="draft.columns.includes(column) && draft.columns[position] !== column"
              >
                {{ column }}
              </option>
            </select></label
          >
          <button
            type="button"
            class="icon-button"
            :aria-label="`Remove index column ${position + 1}`"
            :disabled="saving"
            @click="draft.columns.splice(position, 1)"
          >
            <AppIcon name="minus" :size="15" />
          </button>
        </div>
        <button
          type="button"
          class="button secondary small"
          :disabled="saving || draft.columns.length >= Math.min(32, table.columns.length)"
          @click="draft.columns.push('')"
        >
          <AppIcon name="plus" :size="14" />Add column
        </button>
        <p v-if="original" class="schema-section-note">
          Editing rebuilds this index. Large tables may take longer.
        </p>
      </template>
      <div class="index-actions">
        <button
          type="button"
          class="button secondary small"
          :disabled="saving"
          @click="draft = undefined"
        >
          Cancel
        </button>
        <button type="button" class="button primary small" :disabled="!valid" @click="save">
          {{ saving ? 'Applying…' : deleting ? 'Delete index' : 'Apply index' }}
        </button>
      </div>
    </div>
    <div v-if="error" role="alert" class="schema-field-validation">
      <p>{{ error }}</p>
      <button v-if="!draft" type="button" class="text-button" @click="load">Try again</button>
    </div>
  </section>
</template>

<style scoped>
.index-row,
.index-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}
.index-row {
  justify-content: space-between;
  margin-top: 8px;
}
.index-row > div:first-child {
  min-width: 0;
  overflow-wrap: anywhere;
}
.index-draft {
  display: grid;
  gap: 12px;
  margin-top: 12px;
}
.index-draft label {
  display: grid;
  gap: 6px;
}
.index-draft .schema-field-check {
  display: flex;
}
</style>
