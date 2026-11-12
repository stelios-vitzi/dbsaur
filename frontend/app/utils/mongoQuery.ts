export function mongoFind(collection: string, filter: string, sort: string, projection: string) {
  const object = (value: string, label: string) => {
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
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed))
      throw new Error(`${label} must be a JSON object.`)
    return parsed
  }
  if (!collection) throw new Error('Select a collection first.')
  return JSON.stringify({
    collection,
    filter: object(filter, 'Filter'),
    sort: object(sort, 'Sort'),
    projection: object(projection, 'Projection'),
  })
}
