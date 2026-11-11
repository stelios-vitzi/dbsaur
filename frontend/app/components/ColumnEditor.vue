<script setup lang="ts">
import { computed, onMounted, ref, useTemplateRef } from 'vue'
import AppIcon from './AppIcon.vue'
import { providerOptions } from '../providers'
import { DatabaseChangeCancelled } from '../utils/databaseChange'
import type { ColumnChange, ColumnInfo, Mutation, TableInfo } from '../../shared/database'

const props = defineProps<{
  provider: string
  database: string
  table: TableInfo
  column: ColumnInfo & { definition: NonNullable<ColumnInfo['definition']> }
  apply: (mutation: Mutation) => Promise<{ affectedRows: number }>
}>()
const emit = defineEmits<{ close: []; saved: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const original = props.column.definition
const special = !!original.extra && original.extra.toUpperCase() !== 'DEFAULT_GENERATED'
const action = ref<'definition' | 'rename' | 'drop'>(special ? 'rename' : 'definition')
const name = ref(props.column.name)
const dataType = ref(original.dataType)
const nullable = ref(original.nullable)
const hasDefault = ref(original.defaultExpression !== null)
const defaultExpression = ref(original.defaultExpression ?? '')
const characterSet = ref(original.characterSet ?? '')
const collation = ref(original.collation ?? '')
const confirmation = ref('')
const saving = ref(false)
const error = ref('')
const mysql = props.provider === 'mysql' || props.provider === 'mariadb'
const textual = computed(() =>
  /^(?:n?(?:var)?char|(?:tiny|medium|long)?text|character|enum|set|citext)\b/i.test(
    dataType.value.trim(),
  ),
)
const change = computed<ColumnChange>(() => {
  if (action.value === 'rename') return { kind: 'rename', name: name.value.trim() }
  if (action.value === 'drop') return { kind: 'drop' }
  return {
    kind: 'definition',
    dataType: dataType.value.trim(),
    nullable: nullable.value,
    defaultExpression: hasDefault.value ? defaultExpression.value.trim() : null,
    characterSet: mysql
      ? textual.value
        ? characterSet.value.trim() || null
        : null
      : original.characterSet,
    collation: textual.value ? collation.value.trim() || null : null,
  }
})
const canSave = computed(() => {
  if (saving.value) return false
  const pending = change.value
  if (pending.kind === 'drop') return confirmation.value === props.column.name
  if (pending.kind === 'rename')
    return (
      !!pending.name &&
      pending.name !== props.column.name &&
      new TextEncoder().encode(pending.name).length <= providerOptions(props.provider).nameLimit &&
      !/[\u0000-\u001f\u007f]/.test(pending.name)
    )
  return (
    !special &&
    !!pending.dataType &&
    (!hasDefault.value || !!pending.defaultExpression) &&
    (pending.dataType !== original.dataType ||
      pending.nullable !== original.nullable ||
      pending.defaultExpression !== original.defaultExpression ||
      pending.characterSet !== original.characterSet ||
      pending.collation !== original.collation)
  )
})
async function save() {
  if (!canSave.value) return
  saving.value = true
  error.value = ''
  try {
    await props.apply({
      action: 'alterColumn',
      schema: props.table.schema,
      table: props.table.name,
      column: props.column.name,
      expected: original,
      change: change.value,
    })
    emit('saved')
  } catch (cause) {
    if (cause instanceof DatabaseChangeCancelled) {
      emit('close')
      return
    }
    error.value = cause instanceof Error ? cause.message : 'The column change could not be applied.'
  } finally {
    saving.value = false
  }
}
onMounted(() => dialog.value?.showModal())
</script>

<template>
  <dialog
    ref="dialog"
    class="schema-table-editor column-editor"
    aria-labelledby="column-editor-title"
    @cancel="saving && $event.preventDefault()"
    @close="emit('close')"
  >
    <form class="schema-editor-form" @submit.prevent="save">
      <header class="schema-editor-header">
        <div>
          <h2 id="column-editor-title">Edit column</h2>
          <p>{{ database }} / {{ table.schema }}.{{ table.name }} / {{ column.name }}</p>
        </div>
        <button
          type="button"
          class="icon-button"
          aria-label="Close column editor"
          :disabled="saving"
          @click="dialog?.close()"
        >
          <AppIcon name="close" :size="18" />
        </button>
      </header>
      <div class="schema-editor-body">
        <label
          >Action<select v-model="action" :disabled="saving">
            <option value="definition" :disabled="special">Edit definition</option>
            <option value="rename">Rename column</option>
            <option value="drop">Delete column</option>
          </select></label
        >
        <template v-if="action === 'definition'">
          <div class="column-definition-fields">
            <label
              >Data type<input
                v-model="dataType"
                :disabled="saving"
                required
                maxlength="256"
                placeholder="varchar(255)"
                autocomplete="off"
                spellcheck="false"
            /></label>
            <label class="schema-field-check"
              ><input
                v-model="nullable"
                type="checkbox"
                :disabled="saving || column.primaryKey"
              />Allow NULL values</label
            >
          </div>
          <fieldset>
            <legend>Default value</legend>
            <label class="schema-field-check"
              ><input v-model="hasDefault" type="checkbox" :disabled="saving" />Use a default
              expression</label
            >
            <label v-if="hasDefault" class="column-default-expression"
              >Expression<input
                v-model="defaultExpression"
                :disabled="saving"
                required
                maxlength="4096"
                placeholder="'pending', 0, CURRENT_TIMESTAMP"
                spellcheck="false"
                autocomplete="off"
              /><small
                >Use a SQL expression. Put text defaults in single quotes, for example
                'pending'.</small
              ></label
            >
          </fieldset>
          <fieldset>
            <legend>Text encoding</legend>
            <div class="column-definition-fields">
              <label
                >Character set<input
                  v-model="characterSet"
                  :disabled="saving || !mysql || !textual"
                  :placeholder="
                    mysql
                      ? 'Database default'
                      : provider === 'postgres'
                        ? 'Database encoding'
                        : 'Defined by collation'
                  "
                  list="column-character-sets"
                  maxlength="128"
                  autocomplete="off"
                  spellcheck="false"
              /></label>
              <label
                >Collation<input
                  v-model="collation"
                  :disabled="saving || !textual"
                  placeholder="Database default"
                  maxlength="128"
                  autocomplete="off"
                  spellcheck="false"
              /></label>
            </div>
            <p class="schema-section-note">
              {{
                mysql
                  ? 'Character set and collation must be compatible with the column type.'
                  : provider === 'postgres'
                    ? 'Character encoding belongs to the database. Text columns can use a specific collation.'
                    : 'SQL Server controls Unicode and UTF-8 support through the data type and collation.'
              }}
            </p>
          </fieldset>
          <datalist id="column-character-sets">
            <option value="utf8mb4" />
            <option value="ascii" />
            <option value="latin1" />
          </datalist>
          <p class="schema-editor-note">
            Existing values must fit the new definition. Dependent constraints can prevent a change.
          </p>
        </template>
        <template v-else-if="action === 'rename'"
          ><label
            >Column name<input
              v-model="name"
              :disabled="saving"
              required
              :maxlength="providerOptions(provider).nameLimit"
              autocomplete="off"
              spellcheck="false"
          /></label>
          <p class="schema-editor-note">
            Queries and application code that use this column name may need updating.
          </p></template
        >
        <template v-else
          ><p class="column-delete-note">
            Deleting <strong>{{ column.name }}</strong> permanently removes its values from every
            record. Dependencies may prevent deletion. Related key and index definitions can also
            change.
          </p>
          <label
            >Type {{ column.name }} to confirm<input
              v-model="confirmation"
              :disabled="saving"
              autocomplete="off"
              spellcheck="false" /></label
        ></template>
        <p v-if="special" class="schema-editor-note">
          This column has generated, identity, or other special settings. Its definition is managed
          by the database; rename and deletion are available.
        </p>
        <p v-if="error" class="schema-field-validation" role="alert">{{ error }}</p>
      </div>
      <footer class="schema-editor-footer">
        <span>Changes require confirmation before saving.</span>
        <div>
          <button
            type="button"
            class="button secondary small"
            :disabled="saving"
            @click="dialog?.close()"
          >
            Cancel</button
          ><button
            type="submit"
            class="button small"
            :class="action === 'drop' ? 'danger' : 'primary'"
            :disabled="!canSave"
          >
            {{
              saving
                ? 'Saving…'
                : action === 'drop'
                  ? 'Delete column'
                  : action === 'rename'
                    ? 'Rename column'
                    : 'Save definition'
            }}
          </button>
        </div>
      </footer>
    </form>
  </dialog>
</template>

<style scoped>
.column-editor {
  width: min(660px, calc(100vw - 32px));
}
.column-editor label:not(.schema-field-check) {
  display: grid;
  gap: 7px;
  color: var(--secondary);
}
.column-definition-fields {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 18px;
  align-items: center;
}
.column-editor select {
  width: 100%;
  height: 32px;
  padding: 5px 9px;
  color: var(--text);
  background: var(--canvas);
  border: 1px solid var(--raised);
  border-radius: 4px;
  font-size: inherit;
}
fieldset {
  display: grid;
  gap: 14px;
  min-width: 0;
  padding: 14px;
  border: 1px solid var(--raised);
  border-radius: 6px;
}
legend {
  padding-inline: 6px;
  color: var(--text);
  font-weight: 500;
}
.schema-section-note {
  padding: 0;
}
small {
  color: var(--muted);
  font-size: var(--text-xs);
  line-height: 1.5;
}
.column-delete-note {
  padding: 12px;
  color: var(--danger);
  background: var(--danger-soft);
  border-radius: 4px;
  line-height: 1.6;
}
@media (max-width: 520px) {
  .column-definition-fields {
    grid-template-columns: 1fr;
  }
}
</style>
