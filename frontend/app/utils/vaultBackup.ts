const encoder = new TextEncoder()
const aad = encoder.encode('dbsaur-vault-v1')
const iterations = 600_000
const encode = (bytes: Uint8Array) =>
  Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
function decode(value: unknown, length?: number): Uint8Array<ArrayBuffer> {
  if (
    typeof value !== 'string' ||
    !/^(?:[\da-f]{2})+$/i.test(value) ||
    (length && value.length !== length * 2)
  )
    throw new Error('Invalid vault backup.')
  return Uint8Array.from(value.match(/../g)!, (byte) => parseInt(byte, 16))
}
async function key(passphrase: string, salt: Uint8Array<ArrayBuffer>) {
  if (passphrase.length < 12 || passphrase.length > 1024)
    throw new Error('Use a passphrase of 12–1024 characters.')
  const secret = encoder.encode(passphrase)
  try {
    const material = await crypto.subtle.importKey('raw', secret, 'PBKDF2', false, ['deriveKey'])
    return await crypto.subtle.deriveKey(
      { name: 'PBKDF2', hash: 'SHA-256', salt, iterations },
      material,
      { name: 'AES-GCM', length: 256 },
      false,
      ['encrypt', 'decrypt'],
    )
  } finally {
    secret.fill(0)
  }
}
export function recoveryPassphrase() {
  return encode(crypto.getRandomValues(new Uint8Array(32)))
}
export async function encryptVault(profiles: unknown, passphrase: string): Promise<string> {
  const salt = crypto.getRandomValues(new Uint8Array(16))
  const iv = crypto.getRandomValues(new Uint8Array(12))
  const plaintext = encoder.encode(JSON.stringify(profiles))
  try {
    const ciphertext = await crypto.subtle.encrypt(
      { name: 'AES-GCM', iv, additionalData: aad },
      await key(passphrase, salt),
      plaintext,
    )
    return JSON.stringify({
      format: 'dbsaur-vault-v1',
      salt: encode(salt),
      iv: encode(iv),
      ciphertext: encode(new Uint8Array(ciphertext)),
    })
  } finally {
    plaintext.fill(0)
  }
}
export async function decryptVault(text: string, passphrase: string): Promise<unknown> {
  if (text.length > 64 * 1024 * 1024) throw new Error('Vault backup is too large.')
  try {
    const envelope = JSON.parse(text)
    if (envelope.format !== 'dbsaur-vault-v1') throw new Error()
    const plaintext = new Uint8Array(
      await crypto.subtle.decrypt(
        { name: 'AES-GCM', iv: decode(envelope.iv, 12), additionalData: aad },
        await key(passphrase, decode(envelope.salt, 16)),
        decode(envelope.ciphertext),
      ),
    )
    try {
      return JSON.parse(new TextDecoder().decode(plaintext))
    } finally {
      plaintext.fill(0)
    }
  } catch {
    throw new Error('Could not unlock the backup. Check its passphrase and file integrity.')
  }
}
