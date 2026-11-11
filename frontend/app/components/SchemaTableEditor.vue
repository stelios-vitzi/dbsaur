<script setup lang="ts">
import { ref, computed, onMounted, useTemplateRef } from 'vue'
import { useActionFeedback } from '../composables/useToast'
import AppIcon from './AppIcon.vue'

import { DatabaseChangeCancelled } from '../utils/databaseChange'
import { providerOptions, quoteIdentifier } from '../providers'
import type { Mutation, TableInfo } from '../../shared/database'

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
const options = computed(() => providerOptions(props.provider))
const typeSuggestions = computed(() => {
  const common = ['smallint', 'integer', 'bigint', 'varchar(255)', 'text', 'decimal(10,2)', 'date']
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
      : 'public'
const schema = ref(props.table?.schema ?? defaultSchema)
const tableName = ref(props.table?.name ?? 'new_table')
const identityType =
  props.provider === 'sqlserver'
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
const saving = ref(false)
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
  if (fields.value.length >= 50) return
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
  saving.value = true
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
    saving.value = false
  }
}
onMounted(() => dialog.value?.showModal())
</script>

<template>
  <dialog
    ref="dialog"
    class="mutation-dialog schema-table-editor"
    :aria-label="table ? 'Edit table details' : 'Create table'"
    @cancel="saving && $event.preventDefault()"
    @close="emit('close')"
  >
    <form @submit.prevent="save">
      <div class="inspector-heading">
        <h2>{{ table ? 'Edit table details' : 'Create table' }}</h2>
        <button
          type="button"
          class="icon-button"
          aria-label="Close editor"
          :disabled="saving"
          @click="dialog?.close()"
        >
          <AppIcon name="close" :size="18" />
        </button>
      </div>
      <p class="muted">
        {{ database }}<template v-if="table"> / {{ table.schema }}.{{ table.name }}</template>
      </p>
      <p v-if="suggestUniqueColumn" class="muted">
        A unique key for {{ suggestUniqueColumn }} is selected below so other tables can reference
        it. Review and apply this key, then return to the relationship. Existing values must be
        unique.
      </p>
      <template v-if="!table">
        <label
          >Schema<input
            v-model="schema"
            required
            :maxlength="options.nameLimit"
            autocomplete="off"
            spellcheck="false"
        /></label>
        <label
          >Table name<input
            v-model="tableName"
            required
            :maxlength="options.nameLimit"
            autocomplete="off"
            spellcheck="false"
        /></label>
      </template>
      <section v-else class="schema-existing-fields" aria-label="Existing fields">
        <h3>Existing fields</h3>
        <div v-for="column in knownColumns" :key="column" class="schema-existing-field">
          <AppIcon :name="primaryFields.includes(column) ? 'key' : 'table'" :size="14" />
          <span>{{ column }}</span>
          <label v-if="keyMetadataAvailable" class="schema-key-check">
            <input
              type="checkbox"
              :checked="primaryFields.includes(column)"
              :disabled="saving"
              @change="setPrimaryColumn(column, ($event.target as HTMLInputElement).checked)"
            />
            PK
          </label>
          <small v-else-if="table.primaryKeys?.includes(column)">PK</small>
          <small v-if="table.foreignKeys?.some((key) => key.column === column)">FK</small>
        </div>
        <p v-if="!keyMetadataAvailable" class="muted">
          Key details are unavailable from this backend version. Refresh the explorer to edit keys.
        </p>
      </section>
      <section v-if="table" class="schema-key-editor" aria-label="Unique key constraints">
        <div class="schema-fields-heading">
          <h3>Unique keys</h3>
          <button
            type="button"
            class="button secondary small"
            :disabled="saving || !keyMetadataAvailable || uniqueKeys.length >= 32"
            @click="addUniqueKey"
          >
            <AppIcon name="plus" :size="15" /> Add unique key
          </button>
        </div>
        <p class="muted">
          Select one or more fields for each unique key. Remove a key with the minus button.
        </p>
        <p v-if="!uniqueKeys.length" class="muted">No unique keys defined.</p>
        <div v-for="key in uniqueKeys" :key="key.id" class="schema-unique-key">
          <small class="schema-unique-key-name">{{ key.name ?? 'New unique key' }}</small>
          <div class="schema-unique-key-fields">
            <label v-for="column in keyColumns" :key="column" class="schema-key-check">
              <input
                type="checkbox"
                :checked="key.columns.includes(column)"
                :disabled="saving || !keyMetadataAvailable"
                @change="setUniqueColumn(key, column, ($event.target as HTMLInputElement).checked)"
              />
              {{ column }}
            </label>
          </div>
          <button
            type="button"
            class="icon-button schema-field-remove"
            :aria-label="`Remove unique key ${key.id}`"
            title="Remove unique key"
            :disabled="saving || !keyMetadataAvailable"
            @click="uniqueKeys = uniqueKeys.filter((item) => item.id !== key.id)"
          >
            <AppIcon name="minus" :size="16" />
          </button>
        </div>
      </section>
      <section class="schema-new-fields" aria-label="Fields to add">
        <div class="schema-fields-heading">
          <h3>{{ table ? 'Add fields' : 'Fields' }}</h3>
          <button
            type="button"
            class="button secondary small"
            :disabled="saving || fields.length >= 50"
            @click="addField"
          >
            <AppIcon name="plus" :size="15" /> Add field
          </button>
        </div>
        <p v-if="!fields.length" class="muted">Add a field to continue.</p>
        <p
          v-if="fields.some((field) => field.dataType && !validDataType(field.dataType))"
          class="schema-field-validation"
          role="status"
        >
          Choose a supported database type such as integer, varchar(255), or decimal(10,2).
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
            >Type<input
              v-model="field.dataType"
              required
              maxlength="128"
              :disabled="saving"
              placeholder="varchar(255)"
              list="schema-field-type-options"
              autocomplete="off"
              spellcheck="false"
          /></label>
          <label class="schema-field-check">
            <input
              v-model="field.nullable"
              type="checkbox"
              :disabled="saving || field.primaryKey"
            />
            Nullable
          </label>
          <label v-if="canAddPrimaryKey || table" class="schema-field-check">
            <input
              v-model="field.primaryKey"
              type="checkbox"
              :disabled="
                saving ||
                (table && !keyMetadataAvailable) ||
                (!table && fields.some((item, itemIndex) => itemIndex !== index && item.primaryKey))
              "
            />
            Primary key
          </label>
          <label v-if="!table" class="schema-field-check">
            <input v-model="field.unique" type="checkbox" :disabled="saving || field.primaryKey" />
            Unique
          </label>
          <button
            type="button"
            class="icon-button schema-field-remove"
            :aria-label="`Remove field ${index + 1}`"
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
      <p class="muted">
        Changes apply immediately. Data types use the connected database dialect; required fields
        and primary keys cannot contain NULL values.
      </p>
      <p v-if="error" class="mutation-warning" role="alert">{{ error }}</p>
      <div class="mutation-actions">
        <button type="button" class="button secondary" :disabled="saving" @click="dialog?.close()">
          Cancel
        </button>
        <button type="submit" class="button" :disabled="!canSave">
          {{ saving ? 'Applying…' : table ? 'Apply changes' : 'Create table' }}
        </button>
      </div>
    </form>
  </dialog>
</template>
