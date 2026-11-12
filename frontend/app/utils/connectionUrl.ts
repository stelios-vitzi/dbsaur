import type { ConnectionInput } from '../../shared/database'

const protocols: Record<string, { provider: string; port: number }> = {
  'postgres:': { provider: 'postgres', port: 5432 },
  'postgresql:': { provider: 'postgres', port: 5432 },
  'mysql:': { provider: 'mysql', port: 3306 },
  'mariadb:': { provider: 'mariadb', port: 3306 },
  'sqlserver:': { provider: 'sqlserver', port: 1433 },
  'mongodb:': { provider: 'mongodb', port: 27017 },
}

// Never include the supplied URL in errors: it can contain credentials.
export function parseConnectionUrl(value: string): ConnectionInput {
  const invalid = () =>
    new Error('Enter a valid database URL with a host, username, and optional database.')
  try {
    const raw = value.trim()
    if (!raw || /[\s\\]/u.test(raw) || raw.includes('#')) throw invalid()
    const url = new URL(raw)
    const type = protocols[url.protocol]
    if (!type)
      throw new Error(
        'Use a postgres://, postgresql://, mysql://, mariadb://, sqlserver://, or mongodb:// URL. MongoDB SRV URLs are not supported yet.',
      )
    const host = url.hostname.replace(/^\[|\]$/g, '')
    const database = decodeURIComponent(url.pathname.replace(/^\//, ''))
    const username = decodeURIComponent(url.username)
    const password = decodeURIComponent(url.password)
    const port = url.port ? Number(url.port) : type.port
    if (
      !host ||
      (!username.trim() && type.provider !== 'mongodb') ||
      host.includes(',') ||
      database.includes('/') ||
      !Number.isInteger(port) ||
      port < 1 ||
      port > 65535 ||
      host.length > 253 ||
      database.length > 128 ||
      username.length > 128 ||
      password.length > 4096 ||
      [host, database, username, password].some((part) => /[\u0000-\u001f\u007f]/u.test(part))
    )
      throw invalid()
    let ssl = false
    let authSource: string | undefined
    let tlsSpecified = false
    for (const [key, setting] of url.searchParams) {
      if (key === 'authSource' && type.provider === 'mongodb' && authSource === undefined) {
        if (!setting || setting.length > 128 || /[\u0000-\u001f\u007f]/u.test(setting))
          throw invalid()
        authSource = setting
        continue
      }
      if (
        !(
          type.provider === 'mongodb' ? ['tls', 'ssl'] : ['sslmode', 'ssl-mode', 'ssl', 'encrypt']
        ).includes(key) ||
        tlsSpecified
      ) {
        throw new Error(
          'Unsupported or repeated URL option. Enter advanced settings in the form instead.',
        )
      }
      tlsSpecified = true
      if (['verify-full', 'VERIFY_IDENTITY', 'true'].includes(setting)) ssl = true
      else if (['disable', 'DISABLED', 'false'].includes(setting)) ssl = false
      else
        throw new Error(
          'Use verified TLS or disable TLS explicitly. Other TLS modes are not supported.',
        )
    }
    return {
      provider: type.provider,
      host,
      port,
      database,
      username,
      password,
      ssl,
      allDatabases: !database,
      ...(type.provider === 'mongodb' ? { authSource: authSource ?? (database || 'admin') } : {}),
    }
  } catch (cause) {
    if (cause instanceof TypeError || cause instanceof URIError) throw invalid()
    throw cause
  }
}
