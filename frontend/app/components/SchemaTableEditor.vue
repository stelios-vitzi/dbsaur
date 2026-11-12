<script setup lang="ts">
import { ref, computed, onMounted, onScopeDispose, useTemplateRef } from 'vue'
import { useActionFeedback } from '../composables/useToast'
import AppIcon from './AppIcon.vue'
import ColumnEditor from './ColumnEditor.vue'
import TableIndexes from './TableIndexes.vue'
import { $fetch } from '../services/http'

import { DatabaseChangeCancelled } from '../utils/databaseChange'
import { providerOptions, quoteIdentifier } from '../providers'
import type { ColumnInfo, Mutation, TableInfo, TablePage } from '../../shared/database'

interface NewField {
  name: string
  previousName?: string
  dataType: string
  nullable: boolean
  primaryKey: boolean
  unique: boolean
}

interface UniqueKeyDraft {
  id: string
  name: string | null
  columns: string[]
}

const props = defineProps<{
  provider: string
  database: string
  table: TableInfo | null
  suggestUniqueColumn?: string
  apply: (mutation: Mutation) => Promise<{ affectedRows: number }>
}>()
const emit = defineEmits<{ close: []; saved: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const columnDefinitions = ref<ColumnInfo[]>([])
const columnsLoading = ref(false)
const columnError = ref('')
const editingColumn = ref<ColumnInfo & { definition: NonNullable<ColumnInfo['definition']> }>()
const metadataController = new AbortController()
onScopeDispose(() => metadataController.abort())
async function loadColumnDefinitions() {
  if (!props.table) return
  columnsLoading.value = true
  columnError.value = ''
  try {
    const page = await $fetch<TablePage>('/api/rows', {
      query: { schema: props.table.schema, table: props.table.name, page: 0, pageSize: 1 },
      signal: metadataController.signal,
    })
    columnDefinitions.value = page.columns
    if (props.provider !== 'sqlite' && page.columns.some((column) => !column.definition))
      columnError.value =
        'Column definitions are unavailable. Restart the desktop app with the updated backend.'
  } catch {
    if (!metadataController.signal.aborted)
      columnError.value = 'Could not load column definitions. Try again.'
  } finally {
    columnsLoading.value = false
  }
}
function editExistingColumn(name: string) {
  if (saving.value || fields.value.length || pendingKeyMutation.value) return
  const column = columnDefinitions.value.find((item) => item.name === name)
  if (column?.definition) editingColumn.value = { ...column, definition: column.definition }
}
const options = computed(() => providerOptions(props.provider))
const typeSuggestions = computed(() => {
  const common = ['smallint', 'integer', 'bigint', 'varchar(255)', 'text', 'decimal(10,2)', 'date']
  if (props.provider === 'sqlite') return ['INTEGER', 'REAL', 'TEXT', 'BLOB', 'NUMERIC']
  if (props.provider === 'sqlserver')
    return [...common, 'bit', 'nvarchar(255)', 'datetime2', 'uniqueidentifier', 'varbinary(max)']
  if (props.provider === 'mysql' || props.provider === 'mariadb')
    return [...common, 'tinyint', 'boolean', 'datetime', 'json', 'blob']
  return [...common, 'boolean', 'timestamp with time zone', 'uuid', 'jsonb', 'bytea']
})
const defaultSchema =
  props.provider === 'mysql' || props.provider === 'mariadb'
    ? props.database
    : props.provider === 'sqlserver'
      ? 'dbo'
      : props.provider === 'sqlite'
        ? 'main'
        : 'public'
const schema = ref(props.table?.schema ?? defaultSchema)
const tableName = ref(props.table?.name ?? 'new_table')
const identityType =
  props.provider === 'sqlite'
    ? 'INTEGER'
    : props.provider === 'sqlserver'
      ? 'bigint IDENTITY(1,1)'
      : props.provider === 'postgres'
        ? 'bigint GENERATED ALWAYS AS IDENTITY'
        : 'bigint AUTO_INCREMENT'
const fields = ref<NewField[]>(
  props.table
    ? []
    : [
        { name: 'id', dataType: identityType, nullable: false, primaryKey: true, unique: false },
        {
          name: 'name',
          dataType: 'varchar(255)',
          nullable: false,
          primaryKey: false,
          unique: false,
        },
      ],
)
const knownColumns = ref([...(props.table?.columns ?? [])])
const primaryFields = ref([
  ...(props.table?.keyConstraints?.find((key) => key.kind === 'PRIMARY KEY')?.columns ??
    props.table?.primaryKeys ??
    []),
])
let uniqueDraftId = 0
const uniqueKeys = ref<UniqueKeyDraft[]>(
  (props.table?.keyConstraints ?? [])
    .filter((key) => key.kind === 'UNIQUE')
    .map((key) => ({ id: `existing-${key.name}`, name: key.name, columns: [...key.columns] })),
)
if (
  props.suggestUniqueColumn &&
  props.table?.columns.includes(props.suggestUniqueColumn) &&
  !uniqueKeys.value.some(
    (key) => key.columns.length === 1 && key.columns[0] === props.suggestUniqueColumn,
  )
) {
  uniqueKeys.value.push({
    id: 'suggested-reference-key',
    name: null,
    columns: [props.suggestUniqueColumn],
  })
}
const keyMetadataAvailable = computed(
  () =>
    !!props.table &&
    props.provider !== 'sqlite' &&
    Array.isArray(props.table.keyConstraints) &&
    (!props.table.primaryKeys?.length ||
      props.table.keyConstraints.some((key) => key.kind === 'PRIMARY KEY')),
)
const keyColumns = computed(() => [
  ...new Set([
    ...knownColumns.value,
    ...fields.value.map((field) => field.name.trim()).filter((name) => validIdentifier(name)),
  ]),
])
const indexSaving = ref(false)
const savingColumns = ref(false)
const saving = computed(() => savingColumns.value || indexSaving.value)
const error = ref('')
useActionFeedback(error)
const target = computed(
  () =>
    `${quoteIdentifier(schema.value, props.provider)}.${quoteIdentifier(tableName.value, props.provider)}`,
)
const canAddPrimaryKey = computed(() => !props.table)

function validIdentifier(value: string) {
  const trimmed = value.trim()
  return (
    !!trimmed &&
    new TextEncoder().encode(trimmed).length <= options.value.nameLimit &&
    !/[\u0000-\u001f\u007f]/.test(trimmed)
  )
}
const builtInTypes = new Set([
  'smallint',
  'int',
  'integer',
  'bigint',
  'tinyint',
  'mediumint',
  'serial',
  'smallserial',
  'bigserial',
  'real',
  'float',
  'double',
  'double precision',
  'numeric',
  'decimal',
  'number',
  'money',
  'smallmoney',
  'char',
  'nchar',
  'varchar',
  'nvarchar',
  'character',
  'character varying',
  'text',
  'tinytext',
  'mediumtext',
  'longtext',
  'ntext',
  'binary',
  'varbinary',
  'blob',
  'tinyblob',
  'mediumblob',
  'longblob',
  'bytea',
  'bool',
  'boolean',
  'bit',
  'date',
  'time',
  'timetz',
  'timestamp',
  'timestamptz',
  'datetime',
  'datetime2',
  'datetimeoffset',
  'smalldatetime',
  'year',
  'uuid',
  'uniqueidentifier',
  'json',
  'jsonb',
])
function validDataType(value: string) {
  const trimmed = value.trim()
  if (trimmed.toLowerCase() === identityType.toLowerCase()) return true
  const match = trimmed.match(
    /^([a-z]+(?:\s+[a-z]+)*)(?:\(\s*(?:max|\d+(?:\s*,\s*\d+)?)\s*\))?(?:\s+(?:unsigned|with time zone|without time zone))?$/i,
  )
  return !!match && builtInTypes.has(match[1]!.toLowerCase().replace(/\s+/g, ' '))
}
const fieldsValid = computed(() => {
  if ((!fields.value.length && !props.table) || fields.value.length > 50) return false
  const names = fields.value.map((field) => field.name.trim().toLowerCase())
  if (new Set(names).size !== names.length) return false
  if (names.some((name) => knownColumns.value.some((existing) => existing.toLowerCase() === name)))
    return false
  if (!props.table && fields.value.filter((field) => field.primaryKey).length > 1) return false
  if (
    props.table &&
    primaryFields.value.length + fields.value.filter((field) => field.primaryKey).length > 32
  )
    return false
  if (props.table && uniqueKeys.value.some((key) => key.columns.length === 0)) return false
  if (props.table) {
    const available = new Set([
      ...knownColumns.value,
      ...fields.value.map((field) => field.name.trim()).filter(Boolean),
    ])
    if (
      primaryFields.value.some((column) => !available.has(column)) ||
      uniqueKeys.value.some((key) => key.columns.some((column) => !available.has(column)))
    )
      return false
  }
  return fields.value.every(
    (field) =>
      validIdentifier(field.name) && validDataType(field.dataType) && field.dataType.length <= 128,
  )
})
function sameKeyColumns(left: string[], right: string[]) {
  if (left.length !== right.length) return false
  const normalize = (column: string) =>
    props.provider === 'postgres' ? column : column.toLocaleLowerCase()
  return left.every((column, index) => normalize(column) === normalize(right[index]!))
}
function sameUniqueColumns(left: string[], right: string[]) {
  return sameKeyColumns([...left].sort(), [...right].sort())
}
function keyMutation(): Mutation | null {
  if (!props.table || !keyMetadataAvailable.value) return null
  const existingPrimary = props.table.keyConstraints!.find((key) => key.kind === 'PRIMARY KEY')
  const desiredPrimary = [
    ...primaryFields.value,
    ...fields.value.filter((field) => field.primaryKey).map((field) => field.name.trim()),
  ]
  const primaryUnchanged =
    (!existingPrimary && desiredPrimary.length === 0) ||
    (!!existingPrimary && sameKeyColumns(existingPrimary.columns, desiredPrimary))
  const unmatchedUnique = props.table.keyConstraints!.filter((key) => key.kind === 'UNIQUE')
  const desiredUnique = [
    ...uniqueKeys.value.map((key) => key.columns),
    ...fields.value.filter((field) => field.unique).map((field) => [field.name.trim()]),
  ].filter((columns) => columns.length > 0)
  const addUniqueKeys: string[][] = []
  for (const desired of desiredUnique) {
    const index = unmatchedUnique.findIndex((current) =>
      sameUniqueColumns(current.columns, desired),
    )
    if (index >= 0) unmatchedUnique.splice(index, 1)
    else addUniqueKeys.push(desired)
  }
  const dropPrimaryKey = !primaryUnchanged ? (existingPrimary?.name ?? null) : null
  const addPrimaryKey = !primaryUnchanged ? desiredPrimary : []
  if (!dropPrimaryKey && !addPrimaryKey.length && !unmatchedUnique.length && !addUniqueKeys.length)
    return null
  return {
    action: 'alterTableKeys',
    schema: props.table.schema,
    table: props.table.name,
    dropPrimaryKey,
    dropUniqueKeys: unmatchedUnique.map((key) => key.name),
    addPrimaryKey,
    addUniqueKeys,
  }
}
const pendingKeyMutation = computed(() => keyMutation())
const canSave = computed(
  () =>
    !saving.value &&
    fieldsValid.value &&
    (props.table !== null
      ? fields.value.length > 0 || !!pendingKeyMutation.value
      : fields.value.length > 0 &&
        validIdentifier(schema.value) &&
        validIdentifier(tableName.value)),
)

function addField() {
  if (
    fields.value.length >= 50 ||
    (props.provider === 'sqlite' && props.table && fields.value.length)
  )
    return
  const name = `new_field_${fields.value.length + 1}`
  fields.value.push({
    name,
    previousName: name,
    dataType: 'varchar(255)',
    nullable: true,
    primaryKey: false,
    unique: false,
  })
}
function removeField(index: number) {
  if (saving.value) return
  const [removed] = fields.value.splice(index, 1)
  if (props.table && removed) {
    const name = removed.name.trim()
    primaryFields.value = primaryFields.value.filter((column) => column !== name)
    uniqueKeys.value = uniqueKeys.value
      .map((key) => ({
        ...key,
        columns: key.columns.filter((column) => column !== name),
      }))
      .filter((key) => key.columns.length > 0)
  }
}
function columnDefinition(field: NewField) {
  const nullability = field.primaryKey ? ' NOT NULL' : field.nullable ? ' NULL' : ' NOT NULL'
  const inlineKey = props.table
    ? ''
    : field.primaryKey
      ? ' PRIMARY KEY'
      : field.unique
        ? ' UNIQUE'
        : ''
  return `${quoteIdentifier(field.name.trim(), props.provider)} ${field.dataType.trim()}${nullability}${inlineKey}`
}
function statement() {
  if (!props.table) {
    return `CREATE TABLE ${target.value} (\n  ${fields.value.map(columnDefinition).join(',\n  ')}\n);`
  }
  const definitions = fields.value.map(columnDefinition)
  if (props.provider === 'sqlserver')
    return `ALTER TABLE ${target.value}\n  ADD ${definitions.join(',\n  ')};`
  return `ALTER TABLE ${target.value}\n  ${definitions.map((definition) => `ADD COLUMN ${definition}`).join(',\n  ')};`
}
function addUniqueKey() {
  if (saving.value) return
  uniqueKeys.value.push({ id: `new-${++uniqueDraftId}`, name: null, columns: [] })
}
function setUniqueColumn(key: UniqueKeyDraft, column: string, checked: boolean) {
  if (checked && !key.columns.includes(column)) key.columns.push(column)
  else if (!checked) key.columns = key.columns.filter((name) => name !== column)
}
function setPrimaryColumn(column: string, checked: boolean) {
  if (checked && !primaryFields.value.includes(column)) primaryFields.value.push(column)
  else if (!checked) primaryFields.value = primaryFields.value.filter((name) => name !== column)
}
function renameField(field: NewField) {
  const previous = field.previousName
  const next = field.name.trim()
  if (!previous || previous === next) return
  primaryFields.value = primaryFields.value.map((column) => (column === previous ? next : column))
  for (const key of uniqueKeys.value)
    key.columns = key.columns.map((column) => (column === previous ? next : column))
  field.previousName = next
}
async function save() {
  if (!canSave.value) return
  savingColumns.value = true
  error.value = ''
  try {
    if (!props.table) {
      await props.apply({ action: 'execute', sql: statement() })
    } else {
      if (fields.value.length) {
        await props.apply({ action: 'execute', sql: statement() })
        const added = [...fields.value]
        knownColumns.value.push(...added.map((field) => field.name.trim()))
        primaryFields.value.push(
          ...added.filter((field) => field.primaryKey).map((field) => field.name.trim()),
        )
        uniqueKeys.value.push(
          ...added
            .filter((field) => field.unique)
            .map((field, index) => ({
              id: `added-${++uniqueDraftId}-${index}`,
              name: null,
              columns: [field.name.trim()],
            })),
        )
        fields.value = []
      }
      const change = keyMutation()
      if (change) await props.apply(change)
    }
    emit('saved')
  } catch (cause) {
    if (cause instanceof DatabaseChangeCancelled) {
      emit('close')
      return
    }
    error.value = cause instanceof Error ? cause.message : 'Could not apply the table changes.'
  } finally {
    savingColumns.value = false
  }
}
onMounted(() => {
  dialog.value?.showModal()
  void loadColumnDefinitions()
})
</script>

<template>
  <dialog
    ref="dialog"
    class="schema-table-editor"
    :aria-label="table ? 'Edit table schema' : 'Create table'"
    @cancel="saving && $event.preventDefault()"
    @close="emit('close')"
  >
    <form class="schema-editor-form" @submit.prevent="save">
      <header class="schema-editor-header">
        <div>
          <h2>{{ table ? 'Edit table schema' : 'Create table' }}</h2>
          <p>
            {{ database
            }}<template v-if="table"
              ><span class="schema-path-divider">/</span>{{ table.schema }}.{{
                table.name
              }}</template
            >
          </p>
        </div>
        <button
          type="button"
          class="icon-button"
          aria-label="Close schema editor"
          :disabled="saving"
          @click="dialog?.close()"
        >
          <AppIcon name="close" :size="18" />
        </button>
      </header>
      <div class="schema-editor-body">
        <p v-if="suggestUniqueColumn" class="schema-reference-note">
          A unique key for <strong>{{ suggestUniqueColumn }}</strong> is selected below. Apply it
          before returning to the relationship. Existing values must be unique.
        </p>
        <div v-if="!table" class="schema-table-identity">
          <label
            >Schema<input
              v-model="schema"
              required
              :maxlength="options.nameLimit"
              :disabled="saving"
              autocomplete="off"
              spellcheck="false"
          /></label>
          <label
            >Table name<input
              v-model="tableName"
              required
              :maxlength="options.nameLimit"
              :disabled="saving"
              autocomplete="off"
              spellcheck="false"
          /></label>
        </div>
        <div v-if="table" class="schema-editor-layout">
          <section class="schema-existing-fields" aria-label="Existing fields">
            <div class="schema-section-heading">
              <h3>Columns</h3>
              <span>{{ knownColumns.length }} fields</span>
            </div>
            <div class="schema-column-list">
              <div class="schema-column-head" aria-hidden="true">
                <span>Column name</span><span>Primary key</span><span>References</span>
              </div>
              <div v-for="column in knownColumns" :key="column" class="schema-existing-field">
                <button
                  type="button"
                  class="schema-column-name"
                  :title="'Edit column ' + column"
                  :aria-label="'Edit column ' + column"
                  :disabled="
                    saving ||
                    columnsLoading ||
                    !columnDefinitions.find((item) => item.name === column)?.definition ||
                    fields.length > 0 ||
                    !!pendingKeyMutation
                  "
                  @click="editExistingColumn(column)"
                >
                  <AppIcon
                    :name="primaryFields.includes(column) ? 'key' : 'table'"
                    :size="14"
                  /><span>{{ column }}</span
                  ><AppIcon class="schema-column-edit-icon" name="edit" :size="13" />
                </button>
                <label v-if="keyMetadataAvailable" class="schema-key-check schema-primary-check">
                  <input
                    type="checkbox"
                    :checked="primaryFields.includes(column)"
                    :disabled="saving"
                    :aria-label="'Use ' + column + ' in primary key'"
                    @change="setPrimaryColumn(column, ($event.target as HTMLInputElement).checked)"
                  />
                </label>
                <span v-else class="schema-key-status">{{
                  table.primaryKeys?.includes(column) ? 'PK' : '-'
                }}</span>
                <span class="schema-key-status">{{
                  table.foreignKeys?.some((key) => key.column === column) ? 'FK' : '-'
                }}</span>
              </div>
            </div>
            <p v-if="columnsLoading" class="schema-section-note" role="status">
              Loading column definitions…
            </p>
            <div v-if="columnError" class="schema-section-note" role="alert">
              <p>{{ columnError }}</p>
              <button type="button" class="text-button" @click="loadColumnDefinitions">
                Try again
              </button>
            </div>
            <p v-else class="schema-section-note">
              {{
                provider === 'sqlite'
                  ? 'Use Database changes to rename or drop columns. Type and key changes require rebuilding the table.'
                  : fields.length || pendingKeyMutation
                    ? 'Apply or discard field and key changes before editing an existing column.'
                    : 'Select a column to edit its definition, rename it, or delete it.'
              }}
            </p>
            <p v-if="!keyMetadataAvailable" class="schema-section-note">
              {{
                provider === 'sqlite'
                  ? 'SQLite key changes require rebuilding the table. Add one new column at a time.'
                  : 'Refresh the explorer to load key details before editing keys.'
              }}
            </p>
          </section>
          <section class="schema-key-editor" aria-label="Unique key constraints">
            <div class="schema-section-heading">
              <h3>
                Unique keys <span>{{ uniqueKeys.length }}</span>
              </h3>
              <button
                type="button"
                class="button secondary small"
                :disabled="saving || !keyMetadataAvailable || uniqueKeys.length >= 32"
                @click="addUniqueKey"
              >
                <AppIcon name="plus" :size="14" />Add key
              </button>
            </div>
            <p class="schema-section-note">Select the columns that must be unique together.</p>
            <p v-if="!uniqueKeys.length" class="schema-empty">No unique keys defined.</p>
            <div class="schema-unique-keys">
              <div v-for="key in uniqueKeys" :key="key.id" class="schema-unique-key">
                <div class="schema-unique-key-heading">
                  <span class="schema-unique-key-name" :title="key.name ?? 'New unique key'">{{
                    key.name ?? 'New unique key'
                  }}</span>
                  <button
                    type="button"
                    class="icon-button"
                    :aria-label="'Remove unique key ' + (key.name ?? key.id)"
                    title="Remove unique key"
                    :disabled="saving || !keyMetadataAvailable"
                    @click="uniqueKeys = uniqueKeys.filter((item) => item.id !== key.id)"
                  >
                    <AppIcon name="minus" :size="16" />
                  </button>
                </div>
                <div class="schema-unique-key-fields">
                  <label
                    v-for="column in keyColumns"
                    :key="column"
                    class="schema-key-check"
                    :title="column"
                  >
                    <input
                      type="checkbox"
                      :checked="key.columns.includes(column)"
                      :disabled="saving || !keyMetadataAvailable"
                      @change="
                        setUniqueColumn(key, column, ($event.target as HTMLInputElement).checked)
                      "
                    />
                    <span>{{ column }}</span>
                  </label>
                </div>
              </div>
            </div>
          </section>
        </div>
        <TableIndexes
          v-if="table"
          :table="table"
          :provider="provider"
          :disabled="savingColumns || fields.length > 0 || !!pendingKeyMutation || !!editingColumn"
          :apply="apply"
          @busy="indexSaving = $event"
          @saved="emit('saved')"
        />
        <section class="schema-new-fields" aria-label="Fields to add">
          <div class="schema-section-heading">
            <h3>
              {{ table ? 'New columns' : 'Columns' }} <span>{{ fields.length }}</span>
            </h3>
            <button
              type="button"
              class="button secondary small"
              :disabled="
                saving ||
                fields.length >= 50 ||
                (provider === 'sqlite' && !!table && fields.length >= 1)
              "
              @click="addField"
            >
              <AppIcon name="plus" :size="14" />Add field
            </button>
          </div>
          <p v-if="!fields.length" class="schema-empty">
            No new columns. Add a field to extend this table.
          </p>
          <p
            v-if="fields.some((field) => field.dataType && !validDataType(field.dataType))"
            class="schema-field-validation"
            role="status"
          >
            Choose a supported type such as integer, varchar(255), or decimal(10,2).
          </p>
          <div v-for="(field, index) in fields" :key="index" class="schema-new-field">
            <label
              >Field name<input
                v-model="field.name"
                @input="renameField(field)"
                required
                :maxlength="options.nameLimit"
                :disabled="saving"
                autocomplete="off"
                spellcheck="false"
            /></label>
            <label
              >Data type<input
                v-model="field.dataType"
                required
                maxlength="128"
                :disabled="saving"
                placeholder="varchar(255)"
                list="schema-field-type-options"
                autocomplete="off"
                spellcheck="false"
            /></label>
            <div class="schema-new-field-options">
              <label class="schema-field-check"
                ><input
                  v-model="field.nullable"
                  type="checkbox"
                  :disabled="saving || field.primaryKey"
                />Nullable</label
              >
              <label v-if="canAddPrimaryKey || table" class="schema-field-check"
                ><input
                  v-model="field.primaryKey"
                  type="checkbox"
                  :disabled="
                    saving ||
                    (table && !keyMetadataAvailable) ||
                    (!table &&
                      fields.some((item, itemIndex) => itemIndex !== index && item.primaryKey))
                  "
                />Primary key</label
              >
              <label v-if="!table" class="schema-field-check"
                ><input
                  v-model="field.unique"
                  type="checkbox"
                  :disabled="saving || field.primaryKey"
                />Unique</label
              >
            </div>
            <button
              type="button"
              class="icon-button schema-field-remove"
              :aria-label="'Remove field ' + (index + 1)"
              title="Remove field"
              :disabled="saving"
              @click="removeField(index)"
            >
              <AppIcon name="minus" :size="16" />
            </button>
          </div>
          <datalist id="schema-field-type-options">
            <option v-for="type in typeSuggestions" :key="type" :value="type" />
          </datalist>
        </section>
        <p class="schema-editor-note">
          Data types follow the connected database dialect. Required fields and primary keys cannot
          contain NULL values.
        </p>
        <p v-if="error" class="schema-field-validation" role="alert">{{ error }}</p>
      </div>
      <footer class="schema-editor-footer">
        <span>Review your changes before applying.</span>
        <div>
          <button
            type="button"
            class="button secondary small"
            :disabled="saving"
            @click="dialog?.close()"
          >
            Cancel
          </button>
          <button type="submit" class="button primary small" :disabled="!canSave">
            {{ saving ? 'Applying...' : table ? 'Apply changes' : 'Create table' }}
          </button>
        </div>
      </footer>
    </form>
  </dialog>
  <ColumnEditor
    v-if="editingColumn && table"
    :provider="provider"
    :database="database"
    :table="table"
    :column="editingColumn"
    :apply="apply"
    @close="editingColumn = undefined"
    @saved="emit('saved')"
  />
</template>
