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

export function collect({ platform, version, target = '', root = '.' }) {
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
    execFileSync(
      tar,
      ['-czf', resolve(output, archive), '-C', resolve(release, 'bundle/macos'), 'DBsaur.app'],
      { stdio: 'inherit' },
    )
    assets.push(archive)
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

if (process.argv[1] && resolve(process.argv[1]) === resolve(import.meta.filename)) {
  if (process.argv[2] === 'version') {
    const config = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'))
    const version = releaseVersion(config.version, process.env.GITHUB_RUN_NUMBER || '')
    mkdirSync('.artifacts', { recursive: true })
    writeFileSync('.artifacts/desktop-release.json', JSON.stringify({ version }) + '\n')
    if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `version=${version}\n`)
    console.log(version)
  } else if (process.argv[2] === 'collect') {
    collect({
      platform: process.env.RELEASE_PLATFORM,
      version: process.env.RELEASE_VERSION,
      target: process.env.RELEASE_TARGET,
    })
  } else {
    throw new Error('Usage: node scripts/desktop-release.mjs version|collect')
  }
}
