// Only explicit loopback names receive local development defaults. Do not
// resolve DNS or treat private networks as trusted.
export function connectionSafety(host: string, provider = '') {
  const local =
    provider === 'sqlite' ||
    ['localhost', '127.0.0.1', '::1'].includes(
      host
        .trim()
        .replace(/^\[|\]$/g, '')
        .toLowerCase(),
    )
  return { ssl: !local, readOnly: !local }
}
