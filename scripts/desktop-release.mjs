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
    bundle('msi', '.msi')
    copy(join(release, 'dbsaur-desktop.exe'), `DBsaur_${version}_windows-x64.exe`)
  } else if (platform === 'linux-x64') {
    bundle('deb', '.deb')
    bundle('rpm', '.rpm')
    bundle('appimage', '.AppImage')
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
    if (platform === 'windows-x64') {
      const msi = selectPackage(join(release, 'bundle/msi'), '.msi')
      copy(`${msi}.sig`)
    }
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
    ['.msi', ['windows-x86_64-msi']],
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

if (process.argv[1] && resolve(process.argv[1]) === resolve(import.meta.filename)) {
  if (process.argv[2] === 'preflight') {
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
    writeFileSync(
      'dist/latest.json',
      JSON.stringify(
        updateManifest({
          directory: 'dist',
          version: process.env.RELEASE_VERSION,
          repository: process.env.GITHUB_REPOSITORY,
          tag: process.env.RELEASE_TAG,
        }),
        null,
        2,
      ) + '\n',
    )
  } else {
    throw new Error('Usage: node scripts/desktop-release.mjs preflight|version|collect|manifest')
  }
}
