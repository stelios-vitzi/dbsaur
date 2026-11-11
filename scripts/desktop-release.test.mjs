import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
  symlinkSync,
  lstatSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'
import {
  collect,
  selectPackage,
  releaseVersion,
  updateManifest,
  releaseNotes,
  validateSigningConfiguration,
} from './desktop-release.mjs'

test('release preflight requires both signing keys without exposing their contents', () => {
  assert.doesNotThrow(() => validateSigningConfiguration({ RELEASE_SIGNED: 'false' }))
  assert.throws(
    () => validateSigningConfiguration({ RELEASE_SIGNED: 'true' }),
    /TAURI_UPDATER_PUBLIC_KEY, TAURI_SIGNING_PRIVATE_KEY/,
  )
  const configured = {
    RELEASE_SIGNED: 'true',
    TAURI_UPDATER_PUBLIC_KEY: 'public',
    TAURI_SIGNING_PRIVATE_KEY: 'private-do-not-print',
  }
  assert.doesNotThrow(() => validateSigningConfiguration(configured))
  assert.doesNotThrow(() =>
    validateSigningConfiguration({ ...configured, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' }),
  )
  for (const missing of ['TAURI_UPDATER_PUBLIC_KEY', 'TAURI_SIGNING_PRIVATE_KEY']) {
    assert.throws(
      () => validateSigningConfiguration({ ...configured, [missing]: '  ' }),
      (error) => {
        assert.ok(error.message.includes(missing))
        assert.ok(!error.message.includes('private-do-not-print'))
        return true
      },
    )
  }
})

test('version numbers remain valid for Windows and consistent across platforms', () => {
  assert.equal(releaseVersion('0.1.0', '42'), '0.1.42')
  for (const value of ['', '0', '65536', 'NaN', '-1'])
    assert.throws(() => releaseVersion('0.1.0', value))
})

test('updater manifest requires all signed platforms and maps both macOS architectures', () => {
  const directory = mkdtempSync(join(tmpdir(), 'dbsaur-updater-'))
  const options = { directory, version: '0.1.42', repository: 'owner/repo', tag: 'main-42-1' }
  try {
    for (const name of ['DBsaur-setup.exe', 'DBsaur.AppImage', 'DBsaur.app.tar.gz']) {
      writeFileSync(join(directory, name), 'package')
      writeFileSync(join(directory, `${name}.sig`), 'signed\n')
    }
    const manifest = updateManifest(options)
    assert.equal(manifest.version, '0.1.42')
    assert.deepEqual(Object.keys(manifest.platforms).sort(), [
      'darwin-aarch64',
      'darwin-x86_64',
      'linux-x86_64',
      'windows-x86_64',
      'windows-x86_64-nsis',
    ])
    assert.deepEqual(manifest.platforms['darwin-aarch64'], manifest.platforms['darwin-x86_64'])
    assert.equal(
      manifest.platforms['windows-x86_64'].url,
      'https://github.com/owner/repo/releases/download/main-42-1/DBsaur-setup.exe',
    )
    assert.equal(manifest.platforms['windows-x86_64'].signature, 'signed')
    writeFileSync(join(directory, 'DBsaur universal.dmg'), 'disk image')
    const notes = releaseNotes(options)
    assert.equal((notes.match(/\[Download\]/g) || []).length, 3)
    for (const name of ['DBsaur-setup.exe', 'DBsaur.AppImage', 'DBsaur%20universal.dmg'])
      assert.ok(notes.includes(`https://github.com/owner/repo/releases/download/main-42-1/${name}`))
    assert.ok(notes.includes('automatic updates'))
    rmSync(join(directory, 'DBsaur universal.dmg'))
    assert.throws(() => releaseNotes(options), /Expected one .dmg/)
    writeFileSync(join(directory, 'DBsaur.AppImage.sig'), '')
    assert.throws(() => updateManifest(options), /signature/)
    rmSync(join(directory, 'DBsaur.AppImage.sig'))
    assert.throws(() => updateManifest(options), /ENOENT/)
    assert.throws(() => updateManifest({ ...options, version: 'invalid' }), /version/)
    assert.throws(() => updateManifest({ ...options, repository: '../repo' }), /location/)
  } finally {
    assert.equal(join(directory, '..'), tmpdir())
    rmSync(directory, { recursive: true, force: true })
  }
})

for (const platform of ['windows-x64', 'linux-x64', 'macos-universal']) {
  test(`collect ${platform} packages, preserve app permissions, and isolate checksums`, async () => {
    const root = mkdtempSync(join(tmpdir(), 'dbsaur-release-'))
    const target = platform === 'macos-universal' ? 'universal-apple-darwin' : ''
    const release = join(root, 'dist', target, 'release')
    const file = (path, content = 'test') => {
      mkdirSync(join(path, '..'), { recursive: true })
      writeFileSync(path, content)
    }
    try {
      file(join(root, 'LICENSE'))
      file(join(root, 'THIRD_PARTY_NOTICES.md'))
      const fixtures =
        platform === 'windows-x64'
          ? [['nsis', 'DBsaur_0.1.42_x64-setup.exe']]
          : platform === 'linux-x64'
            ? [['appimage', 'DBsaur_0.1.42_amd64.AppImage']]
            : [['dmg', 'DBsaur_0.1.42_universal.dmg']]
      for (const [directory, name] of fixtures) file(join(release, 'bundle', directory, name))
      if (platform === 'windows-x64') file(join(release, 'dbsaur-desktop.exe'))
      // Old build outputs must not become extra release downloads.
      file(join(release, 'bundle/msi/DBsaur.msi'))
      file(join(release, 'bundle/deb/DBsaur.deb'))
      file(join(release, 'bundle/rpm/DBsaur.rpm'))
      if (platform === 'macos-universal') {
        const executable = join(release, 'bundle/macos/DBsaur.app/Contents/MacOS/dbsaur-desktop')
        file(executable)
        if (process.platform !== 'win32') {
          const { chmodSync } = await import('node:fs')
          chmodSync(executable, 0o755)
          symlinkSync('dbsaur-desktop', join(executable, '..', 'launcher'))
        }
      }
      const updateDirectory =
        platform === 'windows-x64' ? 'nsis' : platform === 'linux-x64' ? 'appimage' : 'macos'
      const updateName =
        platform === 'windows-x64'
          ? fixtures[0][1]
          : platform === 'linux-x64'
            ? fixtures[0][1]
            : 'DBsaur.app.tar.gz'
      const updater = join(release, 'bundle', updateDirectory, updateName)
      if (platform === 'macos-universal') file(updater, 'signed archive must remain byte-identical')
      file(`${updater}.sig`, 'signature')
      const assets = collect({ platform, version: '0.1.42', target, root })
      const packages = fixtures.map(([, name]) => name)
      if (platform === 'macos-universal') packages.push('DBsaur_0.1.42_macos-universal.app.tar.gz')
      assert.deepEqual(assets.sort(), [...packages, 'LICENSE.txt', 'THIRD_PARTY_NOTICES.md'].sort())
      const manifest = readFileSync(
        join(root, 'dist/artifacts', `SHA256SUMS-${platform}.txt`),
        'utf8',
      )
      assert.equal(manifest.trim().split('\n').length, assets.length)
      assert.ok(!manifest.includes('SHA256SUMS'))
      for (const name of assets) assert.ok(manifest.includes(`  ${name}\n`))
      if (platform === 'macos-universal' && process.platform !== 'win32') {
        const extracted = join(root, 'extracted')
        mkdirSync(extracted)
        execFileSync('tar', [
          '-xzf',
          join(root, 'dist/artifacts/DBsaur_0.1.42_macos-universal.app.tar.gz'),
          '-C',
          extracted,
        ])
        assert.ok(
          lstatSync(join(extracted, 'DBsaur.app/Contents/MacOS/dbsaur-desktop')).mode & 0o111,
        )
        assert.ok(lstatSync(join(extracted, 'DBsaur.app/Contents/MacOS/launcher')).isSymbolicLink())
      }
      const signedAssets = collect({ platform, version: '0.1.42', target, root, signed: true })
      assert.deepEqual(
        signedAssets.sort(),
        [...assets, `${platform === 'macos-universal' ? packages[1] : packages[0]}.sig`].sort(),
      )
      if (platform === 'macos-universal')
        assert.equal(
          readFileSync(
            join(root, 'dist/artifacts/DBsaur_0.1.42_macos-universal.app.tar.gz'),
            'utf8',
          ),
          'signed archive must remain byte-identical',
        )
      rmSync(`${updater}.sig`)
      assert.throws(
        () => collect({ platform, version: '0.1.42', target, root, signed: true }),
        /ENOENT/,
      )
      // A second package of the same format must fail rather than publish stale output.
      file(
        join(
          release,
          'bundle',
          fixtures[0][0],
          `duplicate${platform === 'windows-x64' ? '-setup.exe' : platform === 'linux-x64' ? '.AppImage' : '.dmg'}`,
        ),
      )
      assert.throws(() =>
        selectPackage(
          join(release, 'bundle', fixtures[0][0]),
          platform === 'windows-x64'
            ? '-setup.exe'
            : platform === 'linux-x64'
              ? '.AppImage'
              : '.dmg',
        ),
      )
    } finally {
      assert.equal(join(root, '..'), tmpdir())
      rmSync(root, { recursive: true, force: true })
    }
  })
}
