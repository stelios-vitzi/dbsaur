<script setup lang="ts">
import { DatabaseChangeCancelled } from '../utils/databaseChange'
import { providerOptions, quoteIdentifier } from '../providers'
import type { ColumnInfo, Mutation, TableInfo } from '../../shared/database'
const props = defineProps<{
  provider: string
  mode: 'insert' | 'update' | 'delete' | 'schema'
  table: TableInfo | null
  columns: ColumnInfo[]
  row: Record<string, unknown> | null
  database: string
  instance: boolean
  initialAction?: 'execute' | 'createDatabase'
  initialSql?: string
  schemaTitle?: string
  apply: (mutation: Mutation) => Promise<{ affectedRows: number }>
}>()
const emit = defineEmits<{ close: []; saved: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
onMounted(() => dialog.value?.showModal())
const saving = ref(false)
const error = ref('')
useActionFeedback(error)
const confirmation = ref('')
const action = ref<'execute' | 'createDatabase' | 'dropDatabase' | 'dropTable'>(
  props.initialAction ?? 'execute',
)
const name = ref('')
const options = computed(() => providerOptions(props.provider))
const quote = (value: string) => quoteIdentifier(value, props.provider)
const exampleSchema =
  props.table?.schema ??
  (props.provider === 'sqlserver'
    ? 'dbo'
    : props.provider === 'postgres'
      ? 'public'
      : props.database)
const defaultSchema =
  props.provider === 'mysql' || props.provider === 'mariadb'
    ? props.database || 'your_database'
    : exampleSchema
const identity =
  props.provider === 'sqlserver'
    ? 'bigint IDENTITY(1,1) PRIMARY KEY'
    : props.provider === 'postgres'
      ? 'bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY'
      : 'bigint AUTO_INCREMENT PRIMARY KEY'
const sql = ref(
  props.initialSql ??
    `CREATE TABLE ${quote(defaultSchema)}.${quote('new_table')} (\n  ${quote('id')} ${identity},\n  ${quote('name')} varchar(255) NOT NULL\n);`,
)
function text(value: unknown) {
  return value === null || value === undefined
    ? ''
    : typeof value === 'object'
      ? JSON.stringify(value)
      : String(value)
}
const fields = reactive(
  Object.fromEntries(
    props.columns.map((column) => [
      column.name,
      {
        mode: (props.mode === 'insert' || column.readOnly
          ? 'omit'
          : props.row?.[column.name] === null
            ? 'null'
            : 'value') as 'omit' | 'null' | 'value',
        value: text(props.row?.[column.name]),
      },
    ]),
  ),
)
const title = computed(
  () =>
    ({
      insert: 'Insert record',
      update: 'Edit record',
      delete: 'Delete record',
      schema:
        action.value === 'execute' ? (props.schemaTitle ?? 'Database changes') : 'Database changes',
    })[props.mode],
)
const target = computed(() =>
  props.table ? `${quote(props.table.schema)}.${quote(props.table.name)}` : '',
)
watch(
  [action, name, sql, fields],
  () => {
    confirmation.value = ''
    error.value = ''
  },
  { deep: true },
)
const changes = computed(() =>
  Object.fromEntries(
    props.columns.flatMap((column) => {
      const field = fields[column.name]!
      if (field.mode === 'omit') return []
      const value = field.mode === 'null' ? null : field.value
      if (
        props.mode === 'update' &&
        value === (props.row?.[column.name] === null ? null : text(props.row?.[column.name]))
      )
        return []
      return [[column.name, value]]
    }),
  ),
)
const canApply = computed(
  () =>
    confirmation.value === 'APPLY' &&
    !saving.value &&
    (props.mode !== 'update' || Object.keys(changes.value).length > 0),
)
async function save() {
  if (!canApply.value) return
  let mutation: Mutation
  if (props.mode === 'schema') {
    if (action.value === 'execute') mutation = { action: 'execute', sql: sql.value }
    else if (action.value === 'dropTable' && props.table)
      mutation = { action: 'dropTable', schema: props.table.schema, table: props.table.name }
    else if (action.value === 'createDatabase' || action.value === 'dropDatabase')
      mutation = { action: action.value, name: name.value }
    else return
  } else {
    if (!props.table) return
    const base = { schema: props.table.schema, table: props.table.name }
    if (props.mode === 'insert') mutation = { ...base, action: 'insert', values: changes.value }
    else if (props.mode === 'delete' && props.row)
      mutation = { ...base, action: 'delete', original: props.row }
    else if (props.mode === 'update' && props.row)
      mutation = { ...base, action: 'update', original: props.row, values: changes.value }
    else return
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
        {{ database
        }}<template
          v-if="
            mode !== 'schema' || (table && action !== 'createDatabase' && action !== 'dropDatabase')
          "
        >
          / {{ target }}</template
        >
      </p>
      <template v-if="mode === 'schema'">
        <label
          >Action<select v-model="action" :disabled="saving">
            <option value="execute">SQL change — create / alter tables and constraints</option>
            <option v-if="table?.kind === 'BASE TABLE'" value="dropTable">Drop {{ target }}</option>
            <option v-if="instance" value="createDatabase">Create database</option>
            <option v-if="instance" value="dropDatabase">Drop database</option>
          </select></label
        >
        <template v-if="action === 'execute'"
          ><label
            >SQL statement<textarea
              v-model="sql"
              class="sql-change"
              :disabled="saving"
              rows="9"
              spellcheck="false"
            />
          </label>
          <p class="muted">
            Run one statement. Define foreign keys, UNIQUE, CHECK, and other constraints using the
            connected database SQL dialect. {{ options.ddlWarning }}
          </p></template
        >
        <label v-else-if="action === 'createDatabase' || action === 'dropDatabase'"
          >Database name<input
            v-model="name"
            :disabled="saving"
            required
            :maxlength="options.nameLimit"
            autocomplete="off"
        /></label>
        <p v-else class="mutation-warning">
          Permanently drop {{ target }} and all its records. Database dependency rules apply.
        </p>
        <p v-if="action === 'dropDatabase'" class="mutation-warning">
          Permanently delete the named database and all its contents. Switch away from it and
          disconnect its clients first.
        </p>
      </template>
      <template v-else-if="mode !== 'delete'">
        <p class="muted">
          {{ options.recordHelp }} Omit generated fields; omitted insert fields use their database
          defaults.
        </p>
        <div class="mutation-fields">
          <div v-for="column in columns" :key="column.name" class="mutation-field">
            <label :for="`edit-${column.name}`"
              >{{ column.name }}
              <span class="muted"
                >{{ column.dataType }}{{ column.nullable ? '' : ' · required' }}</span
              ></label
            >
            <p v-if="column.readOnly" class="muted">Generated by the database</p>
            <p v-else-if="column.defaultValue" class="muted">Default: {{ column.defaultValue }}</p>
            <select
              v-model="fields[column.name]!.mode"
              :disabled="saving || column.readOnly"
              :aria-label="`${column.name} value mode`"
            >
              <option value="omit">
                {{ mode === 'insert' ? 'Default / omit' : 'Keep original' }}
              </option>
              <option value="value">Value</option>
              <option v-if="column.nullable" value="null">NULL</option>
            </select>
            <textarea
              v-if="fields[column.name]!.mode === 'value'"
              :id="`edit-${column.name}`"
              v-model="fields[column.name]!.value"
              rows="2"
              :disabled="saving || column.readOnly"
              spellcheck="false"
            />
          </div>
        </div>
        <p v-if="mode === 'update'" class="muted">
          {{ Object.keys(changes).length }} changed fields. Concurrent edits or ambiguous duplicate
          records block the save.
        </p>
      </template>
      <template v-else
        ><p class="mutation-warning">
          Permanently delete this record. Foreign key rules may block the deletion or cascade it to
          dependent records.
        </p>
        <pre class="mutation-record">{{ JSON.stringify(row, null, 2) }}</pre>
      </template>
      <p v-if="provider !== 'postgres' && (mode === 'update' || mode === 'delete')" class="muted">
        Updating or deleting requires a primary key and an unchanged record.
      </p>
      <p v-if="mode === 'update'" class="mutation-warning">
        Database triggers and foreign key update rules also apply to this change.
      </p>
      <label
        >Type APPLY to confirm this change<input
          v-model="confirmation"
          :disabled="saving"
          autocomplete="off"
          spellcheck="false"
      /></label>
      <div class="mutation-actions">
        <button type="button" class="button secondary" :disabled="saving" @click="emit('close')">
          Cancel</button
        ><button type="submit" class="button" :disabled="!canApply">
          {{ saving ? 'Applying…' : mode === 'delete' ? 'Delete record' : 'Apply change' }}
        </button>
      </div>
    </form>
  </dialog>
</template>
