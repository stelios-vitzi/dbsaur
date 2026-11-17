export interface MapPreferences {
  enabled: boolean
  url: string
  attribution: string
}
const key = 'dbsaur.map-preferences.v1'
export function validateMapUrl(value: string): string {
  const placeholders = ['{z}', '{x}', '{y}'].filter((part) => value.includes(part)).length
  if (placeholders !== 0 && placeholders !== 3)
    throw new Error('XYZ tile URLs must contain {z}, {x}, and {y}.')
  const url = new URL(value.replaceAll('{z}', '0').replaceAll('{x}', '0').replaceAll('{y}', '0'))
  if (
    !['https:', 'http:'].includes(url.protocol) ||
    url.username ||
    url.password ||
    value.length > 2048
  )
    throw new Error('Use an HTTP(S) style JSON or XYZ tile URL without credentials.')
  return value.trim()
}
export function loadMapPreferences(): MapPreferences {
  try {
    const value = JSON.parse(localStorage.getItem(key) || 'null')
    if (value && typeof value.url === 'string')
      return {
        enabled: value.enabled === true,
        url: value.url ? validateMapUrl(value.url) : '',
        attribution: typeof value.attribution === 'string' ? value.attribution : '',
      }
  } catch {
    /* Offline by default, including unavailable storage. */
  }
  return { enabled: false, url: '', attribution: '' }
}
export function saveMapPreferences(value: MapPreferences) {
  const url = value.url.trim()
  if (url) validateMapUrl(url)
  if (value.enabled && !url)
    throw new Error('Enter a tile or style URL before enabling external maps.')
  localStorage.setItem(key, JSON.stringify({ ...value, url }))
}
