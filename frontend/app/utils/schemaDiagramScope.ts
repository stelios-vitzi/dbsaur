import type { TableInfo } from '../../shared/database'

/** Databases at or below this size open with every table in view. */
export const fullDiagramTableLimit = 24

export function diagramTableKey(table: Pick<TableInfo, 'schema' | 'name'>) {
  return JSON.stringify([table.schema, table.name])
}

/** Undirected foreign-key neighbours for each table, ignoring self references. */
export function relationshipAdjacency(tables: TableInfo[]) {
  const adjacency = new Map(tables.map((table) => [diagramTableKey(table), new Set<string>()]))
  for (const table of tables) {
    const source = diagramTableKey(table)
    for (const key of table.foreignKeys ?? []) {
      const target = diagramTableKey({ schema: key.referencedSchema, name: key.referencedTable })
      if (target === source || !adjacency.has(target)) continue
      adjacency.get(source)!.add(target)
      adjacency.get(target)!.add(source)
    }
  }
  return adjacency
}

/** The table plus every table reachable through at most `depth` relationships. */
export function relationshipNeighborhood(
  adjacency: Map<string, Set<string>>,
  start: string,
  depth: number,
) {
  const reached = new Set([start])
  let frontier = [start]
  for (let hop = 0; hop < depth && frontier.length; hop += 1) {
    const next: string[] = []
    for (const key of frontier) {
      for (const neighbor of adjacency.get(key) ?? []) {
        if (reached.has(neighbor)) continue
        reached.add(neighbor)
        next.push(neighbor)
      }
    }
    frontier = next
  }
  return reached
}

/** Most connected tables first; good starting points for a large schema. */
export function connectedStartingTables(
  tables: TableInfo[],
  adjacency: Map<string, Set<string>>,
  limit: number,
) {
  return tables
    .map((table) => ({ table, links: adjacency.get(diagramTableKey(table))?.size ?? 0 }))
    .filter((entry) => entry.links > 0)
    .sort(
      (a, b) =>
        b.links - a.links ||
        a.table.schema.localeCompare(b.table.schema) ||
        a.table.name.localeCompare(b.table.name),
    )
    .slice(0, limit)
}

/** Name matches before column-only matches, capped for a compact picker. */
export function findDiagramTables(tables: TableInfo[], query: string, limit: number) {
  const needle = query.trim().toLowerCase()
  if (!needle) return []
  const byName: TableInfo[] = []
  const byColumn: TableInfo[] = []
  for (const table of tables) {
    if (`${table.schema}.${table.name}`.toLowerCase().includes(needle)) byName.push(table)
    else if (table.columns.some((column) => column.toLowerCase().includes(needle)))
      byColumn.push(table)
  }
  return [...byName, ...byColumn].slice(0, limit)
}
