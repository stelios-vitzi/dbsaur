export interface Provider {
  id: string
  name: string
}
export interface ConnectionInput {
  provider: string
  host: string
  port: number
  database: string
  username: string
  password: string
  ssl: boolean
  allDatabases?: boolean
  readOnly?: boolean
  tlsCa?: string
  tlsCert?: string
  tlsKey?: string
  saveName?: string
}
export interface SavedConnection {
  id: string
  name: string
}
export interface Session {
  provider: string
  database: string
  host: string
  expiresIn: number
  allDatabases: boolean
  readOnly: boolean
}
export interface TableInfo {
  schema: string
  name: string
  kind: string
  columns: string[]
  primaryKeys?: string[]
  keyConstraints?: KeyConstraintInfo[]
  referenceableColumns?: string[]
  foreignKeys?: ForeignKeyInfo[]
}
export interface KeyConstraintInfo {
  name: string
  kind: 'PRIMARY KEY' | 'UNIQUE'
  columns: string[]
}
export interface ForeignKeyInfo {
  name: string
  column: string
  referencedSchema: string
  referencedTable: string
  referencedColumn: string
}
export interface RelationshipChange {
  constraintName?: string
  foreignSchema: string
  foreignTable: string
  foreignColumn: string
  referencedSchema: string
  referencedTable: string
  referencedColumn: string
}
export interface ColumnInfo {
  name: string
  dataType: string
  nullable: boolean
  primaryKey: boolean
  readOnly: boolean
  defaultValue: string | null
  definition?: ColumnDefinition
}
export interface ColumnDefinition {
  dataType: string
  nullable: boolean
  defaultExpression: string | null
  characterSet: string | null
  collation: string | null
  comment: string
  extra: string
  defaultConstraint: string | null
}
export type ColumnChange =
  | { kind: 'rename'; name: string }
  | { kind: 'drop' }
  | {
      kind: 'definition'
      dataType: string
      nullable: boolean
      defaultExpression: string | null
      characterSet: string | null
      collation: string | null
    }
export interface TablePage {
  columns: ColumnInfo[]
  rows: Record<string, unknown>[]
  hasMore: boolean
  page: number
  pageSize: number
  totalRows?: string | null
  editing?: {
    schema: string
    table: string
    columns: Record<string, string>
    originals: Record<string, unknown>[]
  } | null
}

export interface IndexInfo {
  name: string
  columns: string[]
  unique: boolean
  editable: boolean
}

export type Mutation =
  | {
      action: 'alterIndex'
      schema: string
      table: string
      expected: IndexInfo | null
      index: IndexInfo | null
    }
  | {
      action: 'alterColumn'
      schema: string
      table: string
      column: string
      expected: ColumnDefinition
      change: ColumnChange
    }
  | { action: 'insert'; schema: string; table: string; values: Record<string, string | null> }
  | {
      action: 'update'
      schema: string
      table: string
      original: Record<string, unknown>
      values: Record<string, string | null>
    }
  | { action: 'delete'; schema: string; table: string; original: Record<string, unknown> }
  | { action: 'createDatabase' | 'dropDatabase'; name: string }
  | { action: 'dropTable'; schema: string; table: string }
  | {
      action: 'alterTableKeys'
      schema: string
      table: string
      dropPrimaryKey: string | null
      dropUniqueKeys: string[]
      addPrimaryKey: string[]
      addUniqueKeys: string[][]
    }
  | { action: 'execute'; sql: string }

export interface DatabaseSettings {
  provider: string
  database: string
  properties: Record<string, unknown>
  settings: DatabaseSetting[]
  canEdit: boolean
}
export interface DatabaseSetting {
  name: string
  setting: string | null
  databaseValue: string | null
  editable: boolean
  databaseScoped: boolean
  unit: string | null
  category: string
  short_desc: string
  context: string
  vartype: string
  min_val: string | null
  max_val: string | null
  enumvals: string[] | null
  pending_restart: boolean
  source: string
}
export interface ContextAction {
  label: string
  disabled?: boolean
  run: () => void | Promise<void>
}
