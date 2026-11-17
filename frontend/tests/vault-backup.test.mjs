import assert from 'node:assert/strict'
import { test } from 'node:test'
import { encryptVault, decryptVault, recoveryPassphrase } from '../app/utils/vaultBackup.ts'

test('vault backup round trips exact values, randomizes encryption and authenticates contents', async () => {
  const profiles = [{ name: 'Remote', input: { password: 'never persist plaintext', readOnly: true, port: 5432 } }]
  const passphrase = recoveryPassphrase()
  assert.equal(passphrase.length, 64)
  const first = await encryptVault(profiles, passphrase)
  assert.ok(!first.includes('never persist plaintext'))
  assert.notEqual(first, await encryptVault(profiles, passphrase))
  assert.deepEqual(await decryptVault(first, passphrase), profiles)
  await assert.rejects(decryptVault(first, 'incorrect passphrase'), /Could not unlock/)
  const modified = JSON.parse(first)
  modified.ciphertext = (modified.ciphertext.startsWith('00') ? '01' : '00') + modified.ciphertext.slice(2)
  await assert.rejects(decryptVault(JSON.stringify(modified), passphrase), /Could not unlock/)
  await assert.rejects(encryptVault(profiles, 'short'), /12/)
})
