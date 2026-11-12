import type { ConnectionInput } from '../../shared/database'

// Only opaque connection identifiers and database preferences belong in browser storage.
export function createConnectionPreferences(storage: () => Storage = () => localStorage) {
  let activeConnectionKey: string | undefined
  const activeConnectionStorageKey = 'webdb:active-connection-key'

  function setActiveConnectionKey(key: string | undefined) {
    activeConnectionKey = key
    try {
      if (key) storage().setItem(activeConnectionStorageKey, key)
      else storage().removeItem(activeConnectionStorageKey)
    } catch {
      /* The session continues to work if the browser blocks local storage. */
    }
  }

  function storedDatabase(key: string) {
    try {
      return storage().getItem(`webdb:last-database:${key}`) || undefined
    } catch {
      return undefined
    }
  }
  function rememberDatabase(database: string, key = activeConnectionKey) {
    if (!key || !database) return
    try {
      storage().setItem(`webdb:last-database:${key}`, database)
    } catch {
      /* Database preference remains in the active session if storage is unavailable. */
    }
  }
  function forgetDatabase(key: string) {
    try {
      storage().removeItem(`webdb:last-database:${key}`)
    } catch {
      /* Nothing else depends on this local preference. */
    }
  }
  function restore() {
    try {
      activeConnectionKey = storage().getItem(activeConnectionStorageKey) || undefined
    } catch {
      activeConnectionKey = undefined
    }
  }
  return {
    setActiveConnectionKey,
    storedDatabase,
    rememberDatabase,
    forgetDatabase,
    restore,
    get activeKey() {
      return activeConnectionKey
    },
  }
}

export async function connectionKey(input: ConnectionInput, savedId?: string) {
  if (savedId) return `saved:${savedId}`
  if (input.provider === 'demo' || !globalThis.crypto?.subtle) return undefined
  const identity = JSON.stringify([
    input.provider,
    input.host.trim().toLowerCase(),
    input.port,
    input.username,
    ...(input.provider === 'mongodb' ? [input.authSource || 'admin'] : []),
  ])
  try {
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(identity))
    return `connection:${Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('')}`
  } catch {
    return undefined
  }
}
