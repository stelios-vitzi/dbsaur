function mongoJson(value: string, label: string): unknown {
  let parsed: unknown
  try {
    parsed = JSON.parse(value || '{}', (_key, item: unknown) => {
      if (
        typeof item === 'number' &&
        (!Number.isFinite(item) || (Number.isInteger(item) && !Number.isSafeInteger(item)))
      )
        throw new Error('Use Extended JSON for large numbers.')
      return item
    })
  } catch {
    throw new Error(
      `${label} must be valid JSON with safe numbers. Use double quotes around field names and Extended JSON for large numbers.`,
    )
  }
  return parsed
}
export function mongoObject(value: string, label: string): Record<string, unknown> {
  const parsed = mongoJson(value, label)
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed))
    throw new Error(`${label} must be a JSON object.`)
  return parsed as Record<string, unknown>
}
export function mongoFind(collection: string, filter: string, sort: string, projection: string) {
  if (!collection) throw new Error('Select a collection first.')
  return JSON.stringify({
    collection,
    filter: mongoObject(filter, 'Filter'),
    sort: mongoObject(sort, 'Sort'),
    projection: mongoObject(projection, 'Projection'),
  })
}

export function mongoAggregate(collection: string, pipeline: string) {
  if (!collection) throw new Error('Select a collection first.')
  const value = mongoJson(pipeline, 'Pipeline')
  if (
    !Array.isArray(value) ||
    value.some(
      (stage) =>
        !stage ||
        typeof stage !== 'object' ||
        Array.isArray(stage) ||
        Object.keys(stage).length !== 1,
    )
  )
    throw new Error('Pipeline must be an array of single-operator stage objects.')
  return JSON.stringify({ collection, pipeline: value })
}
