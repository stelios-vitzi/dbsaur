import {
  appendFileSync,
  copyFileSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { basename, join, resolve } from 'node:path'

// Also exported so package selection and collection can be checked on any host.
export function selectPackage(directory, extension) {
  const matches = readdirSync(directory).filter(
    (name) => name.endsWith(extension) && statSync(join(directory, name)).isFile(),
  )
  if (matches.length !== 1)
    throw new Error(`Expected one ${extension} in ${directory}; found ${matches.length}`)
  return join(directory, matches[0])
}

export function collect({ platform, version, target = '', root = '.', signed = false }) {
  if (!['windows-x64', 'linux-x64', 'macos-universal'].includes(platform))
    throw new Error(`Unsupported release platform: ${platform}`)
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('A numeric release version is required')
  const release = join(root, 'dist', target, 'release')
  const output = join(root, 'dist/artifacts')
  mkdirSync(output, { recursive: true })
  const assets = []
  const copy = (source, name = basename(source)) => {
    copyFileSync(source, join(output, name))
    assets.push(name)
  }
  const bundle = (directory, extension) =>
    copy(selectPackage(join(release, 'bundle', directory), extension))
  if (platform === 'windows-x64') {
    bundle('nsis', '-setup.exe')
  } else if (platform === 'linux-x64') {
    bundle('appimage', '.AppImage')
    bundle('deb', '.deb')
    bundle('rpm', '.rpm')
  } else {
    bundle('dmg', '.dmg')
    const app = join(release, 'bundle/macos/DBsaur.app')
    if (!statSync(join(app, 'Contents/MacOS/dbsaur-desktop')).isFile())
      throw new Error('Incomplete macOS app bundle')
    const archive = `DBsaur_${version}_macos-universal.app.tar.gz`
    // Git Bash's GNU tar interprets a Windows drive letter as a remote host.
    const tar =
      process.platform === 'win32'
        ? join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'tar.exe')
        : 'tar'
    if (!signed)
      execFileSync(
        tar,
        ['-czf', resolve(output, archive), '-C', resolve(release, 'bundle/macos'), 'DBsaur.app'],
        { stdio: 'inherit' },
      )
    if (signed) copy(selectPackage(join(release, 'bundle/macos'), '.app.tar.gz'), archive)
    else assets.push(archive)
  }
  if (signed) {
    const directory =
      platform === 'windows-x64' ? 'nsis' : platform === 'linux-x64' ? 'appimage' : 'macos'
    const extension =
      platform === 'windows-x64'
        ? '-setup.exe'
        : platform === 'linux-x64'
          ? '.AppImage'
          : '.app.tar.gz'
    const source = selectPackage(join(release, 'bundle', directory), extension)
    const artifact =
      platform === 'macos-universal'
        ? `DBsaur_${version}_macos-universal.app.tar.gz`
        : basename(source)
    copy(`${source}.sig`, `${artifact}.sig`)
    if (!readFileSync(join(output, `${artifact}.sig`), 'utf8').trim())
      throw new Error('Updater signature must not be empty')
  }
  copy(join(root, 'LICENSE'), 'LICENSE.txt')
  copy(join(root, 'THIRD_PARTY_NOTICES.md'))
  const checksums =
    assets
      .sort()
      .map(
        (name) =>
          `${createHash('sha256')
            .update(readFileSync(join(output, name)))
            .digest('hex')}  ${name}`,
      )
      .join('\n') + '\n'
  writeFileSync(join(output, `SHA256SUMS-${platform}.txt`), checksums)
  return assets
}

export function releaseVersion(base, runNumber) {
  if (!/^\d+$/.test(runNumber) || Number(runNumber) < 1 || Number(runNumber) > 65535)
    throw new Error('Release run number must be between 1 and 65535 (Windows patch version limit)')
  const [major, minor] = base.split('.')
  return `${major}.${minor}.${Number(runNumber)}`
}

export function isVersionBump(previous, current) {
  const parse = (version) => {
    if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('Invalid source version')
    return version.split('.').map(BigInt)
  }
  const before = parse(previous)
  const after = parse(current)
  for (let index = 0; index < before.length; index++) {
    if (after[index] !== before[index]) return after[index] > before[index]
  }
  return false
}

export function validateSigningConfiguration(env) {
  if (env.RELEASE_SIGNED !== 'true') return
  const missing = ['TAURI_UPDATER_PUBLIC_KEY', 'TAURI_SIGNING_PRIVATE_KEY'].filter(
    (name) => !env[name]?.trim(),
  )
  if (missing.length)
    throw new Error(
      `Missing GitHub Actions signing configuration: ${missing.join(', ')}. ` +
        'Set the public key as a repository variable and the private key as a repository secret. ' +
        'See docs/desktop-updates.md. No release can be published until signing is configured.',
    )
}

export function updateManifest({ directory, version, repository, tag }) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error('Invalid updater version')
  if (!/^[\w-]+\/[\w-][\w.-]*$/.test(repository) || !/^[\w-][\w.-]*$/.test(tag))
    throw new Error('Invalid release location')
  const platforms = {}
  for (const [extension, targets] of [
    ['-setup.exe', ['windows-x86_64', 'windows-x86_64-nsis']],
    ['.AppImage', ['linux-x86_64']],
    ['.app.tar.gz', ['darwin-x86_64', 'darwin-aarch64']],
  ]) {
    const artifact = selectPackage(directory, extension)
    const signature = readFileSync(`${artifact}.sig`, 'utf8').trim()
    if (!signature) throw new Error('Updater signature must not be empty')
    for (const target of targets)
      platforms[target] = {
        url: `https://github.com/${repository}/releases/download/${tag}/${encodeURIComponent(basename(artifact))}`,
        signature,
      }
  }
  return { version, platforms }
}

export function releaseNotes({ directory, repository, tag, version }) {
  // Validate the release location and signed updater assets before linking downloads.
  updateManifest({ directory, repository, tag, version })
  const downloads = [
    ['Windows x64', '-setup.exe', 'Run the setup installer.'],
    ['Linux x64 (Debian/Ubuntu)', '.deb', 'Install with `sudo apt install ./<filename>.deb`.'],
    ['Linux x64 (Fedora/RPM)', '.rpm', 'Install with `sudo dnf install ./<filename>.rpm`.'],
    ['Linux x64 (portable)', '.AppImage', 'Make the AppImage executable, then run it.'],
    ['macOS (Intel and Apple Silicon)', '.dmg', 'Open the DMG and drag DBsaur into Applications.'],
  ].map(([platform, extension, instructions]) => {
    const name = basename(selectPackage(directory, extension))
    const url = `https://github.com/${repository}/releases/download/${tag}/${encodeURIComponent(name)}`
    return `| ${platform} | [Download](${url}) | ${instructions} |`
  })
  return [
    '## Downloads',
    '',
    'Choose the download for your operating system:',
    '',
    '| Platform | Download | Installation |',
    '| --- | --- | --- |',
    ...downloads,
    '',
    'The remaining assets support automatic updates (`latest.json`, `.sig`, and the macOS `.app.tar.gz`), download verification (`SHA256SUMS-*.txt`), and license notices. You do not need to download them manually.',
    '',
    'If you previously installed the Windows MSI, uninstall that installation before switching to the setup installer. Existing DEB/RPM installations should also be removed with their package manager before switching to AppImage.',
    '',
  ].join('\n')
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(import.meta.filename)) {
  if (process.argv[2] === 'detect-version') {
    const before = process.env.RELEASE_BEFORE || ''
    if (!/^[a-f0-9]{40}$/.test(before)) throw new Error('Invalid previous push commit')
    // A newly created branch has no previous version to compare; do not publish it.
    let bumped = false
    if (!/^0+$/.test(before)) {
      const previous = JSON.parse(
        execFileSync('git', ['show', `${before}:src-tauri/tauri.conf.json`], { encoding: 'utf8' }),
      )
      const current = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'))
      bumped = isVersionBump(previous.version, current.version)
    }
    if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `bumped=${bumped}\n`)
    console.log(`Source version increased: ${bumped}`)
  } else if (process.argv[2] === 'preflight') {
    validateSigningConfiguration(process.env)
    console.log('Release signing configuration check passed.')
  } else if (process.argv[2] === 'version') {
    const config = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'))
    const version = releaseVersion(config.version, process.env.GITHUB_RUN_NUMBER || '')
    mkdirSync('.artifacts', { recursive: true })
    const pubkey = process.env.TAURI_UPDATER_PUBLIC_KEY?.trim()
    const signing = process.env.RELEASE_SIGNED === 'true'
    if (signing && !pubkey)
      throw new Error('TAURI_UPDATER_PUBLIC_KEY is required for release builds')
    const override = {
      version,
      ...(signing
        ? {
            bundle: { createUpdaterArtifacts: true },
            plugins: { updater: { ...config.plugins.updater, pubkey } },
          }
        : {}),
    }
    writeFileSync('.artifacts/desktop-release.json', JSON.stringify(override) + '\n')
    if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\n`)
    console.log(version)
  } else if (process.argv[2] === 'collect') {
    collect({
      platform: process.env.RELEASE_PLATFORM,
      version: process.env.RELEASE_VERSION,
      target: process.env.RELEASE_TARGET,
      signed: process.env.RELEASE_SIGNED === 'true',
    })
  } else if (process.argv[2] === 'manifest') {
    const options = {
      directory: 'dist',
      version: process.env.RELEASE_VERSION,
      repository: process.env.GITHUB_REPOSITORY,
      tag: process.env.RELEASE_TAG,
    }
    writeFileSync('dist/latest.json', JSON.stringify(updateManifest(options), null, 2) + '\n')
    mkdirSync('.artifacts', { recursive: true })
    writeFileSync('.artifacts/release-notes.md', releaseNotes(options))
  } else {
    throw new Error(
      'Usage: node scripts/desktop-release.mjs detect-version|preflight|version|collect|manifest',
    )
  }
}
