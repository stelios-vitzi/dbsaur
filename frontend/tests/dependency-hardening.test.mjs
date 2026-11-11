import assert from 'node:assert/strict'
import { generateKeyPairSync, sign as cryptoSign } from 'node:crypto'
import { createRequire } from 'node:module'
import test from 'node:test'

const require = createRequire(import.meta.url)
const braces = createRequire(require.resolve('micromatch'))('braces')
const forge = createRequire(require.resolve('listhen'))('node-forge')

test('braces rejects excessive nesting on all public string entry points', () => {
  const pattern = `${'{'.repeat(101)}x${'}'.repeat(101)}`

  assert.throws(() => braces(pattern), /exceeds max depth/)
  assert.throws(() => braces.expand(pattern), /exceeds max depth/)
  assert.throws(() => braces.parse(pattern), /exceeds max depth/)
  assert.throws(() => braces('{{a,b},c}', { maxDepth: 1 }), /exceeds max depth/)
})

test('braces guards direct AST walks and cyclic parent chains', () => {
  let ast = { type: 'text', value: 'x' }
  for (let depth = 0; depth < 101; depth++) {
    ast = { type: 'brace', nodes: [ast] }
  }
  ast = { type: 'root', nodes: [ast] }

  assert.throws(() => braces.compile(ast), /exceeds max depth/)
  assert.throws(() => braces.expand(ast), /exceeds max depth/)
  assert.throws(() => braces.stringify(ast), /exceeds max depth/)

  const first = { type: 'paren', nodes: [] }
  const second = { type: 'text', nodes: [], parent: first }
  first.parent = second
  first.nodes.push(second)
  assert.throws(() => braces.expand(first), /parent chain contains a cycle/)
})

test('node-forge rejects the nested DigestAlgorithm signature forgery', () => {
  const modulus = new forge.jsbn.BigInteger(
    'E932AC92252F585B3A80A4DD76A897C8B7652952FE788F6EC8DD640587A1EE56' +
      '47670A8AD4C2BE0F9FA6E49C605ADF77B5174230AF7BD50E5D6D6D6D28CCF0A8' +
      '86A514CC72E51D209CC772A52EF419F6A953F3135929588EBE9B351FCA61CED7' +
      '8F346FE00DBB6306E5C2A4C6DFC3779AF85AB417371CF34D8387B9B30AE46D7A' +
      '5FF5A655B8D8455F1B94AE736989D60A6F2FD5CADBFFBD504C5A756A2E6BB5CE' +
      'CC13BCA7503F6DF8B52ACE5C410997E98809DB4DC30D943DE4E812A47553DCE5' +
      '4844A78E36401D13F77DC650619FED88D8B3926E3D8E319C80C744779AC5D6AB' +
      'E252896950917476ECE5E8FC27D5F053D6018D91B502C4787558A002B9283DA7',
    16,
  )
  const publicKey = forge.rsa.setPublicKey(modulus, new forge.jsbn.BigInteger('3'))
  const digest = forge.md.sha256.create()
  digest.update('hello world!')
  const forgedSignature = forge.util.hexToBytes(
    'a4ae63dd5e7712b78f4870d0f51e294df5503d4f16c5d27ae33370981fb57f0de49f' +
      '50f3d6a04666774cd984cd13972db9bf8e12bd294ef0ddc916c7c86cbae63efd7b6b' +
      '97885e69760c208a40f1aecc76a90d7af5145177efce1bb55807a8d05c20b1596753' +
      'ba710642fc9acdde6c160232654662c77cc4466c8257a38edb49f894e8845d0fd987' +
      'b857ced88f4b62505a080bd87ef700d35d392a6e8f6fde34250c50b86fae606cb551' +
      '215e8f4813239b77651d5565ad453698c071d48c31e8e526fb4a37610f64b3e1fb8e' +
      '5be5898e408ad08197a0947794a530b54f84485377ce4a7488ed485ce4e5e105dd89' +
      '698a472f390c3b1b76bc16b73276c4d1c81d',
  )

  assert.throws(
    () =>
      publicKey.verify(digest.digest().getBytes(), forgedSignature, undefined, {
        _skipPaddingChecks: true,
      }),
    /DigestInfo value/,
  )
})

test('node-forge continues to verify a valid RSA SHA-256 signature', () => {
  const message = 'dbsaur dependency regression check'
  const { privateKey, publicKey } = generateKeyPairSync('rsa', {
    modulusLength: 2048,
    publicExponent: 65537,
  })
  const signature = cryptoSign('sha256', Buffer.from(message), privateKey)
  const forgePublicKey = forge.pki.publicKeyFromPem(
    publicKey.export({ type: 'spki', format: 'pem' }),
  )
  const digest = forge.md.sha256.create()
  digest.update(message)

  assert.equal(
    forgePublicKey.verify(digest.digest().getBytes(), signature.toString('binary')),
    true,
  )
})
