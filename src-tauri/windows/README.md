# Windows installer

`installer.nsi` is the Tauri CLI **2.12.4** NSIS template, adapted under
Tauri's [MIT license](LICENSE-MIT):
https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.4/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi

The changes use `MULTIUSER_EXECUTIONLEVEL User`, replace the built-in scope page
with `ScopePage`/`ScopePageLeave`, call `ScopeInit`/`un.ScopeInit`, and expand
`DBSAUR_SCOPE_FUNCTIONS` from `scope.nsh`. Tauri's standard `both` mode requests
the highest available privileges before showing any page. This adaptation starts
without elevation and relaunches with `runas` only for an all-users operation.
When upgrading the pinned CLI, compare and update this template as well.

Fresh installs default to the current user. Existing installations retain their
scope for passive/silent updates. `/CurrentUser` and `/AllUsers` explicitly select
the scope; machine operations request elevation if needed. The original process
waits for the elevated process and propagates its exit code. Cancelling UAC leaves
the interactive scope page available. Uninstallers identify the machine scope by
matching their installation directory to the machine registry entry.

Before release, exercise both scopes from an ordinary Windows account: fresh
install, UAC cancellation, upgrade (including passive updater mode), and uninstall.
Also test elevation using a different administrator account. Compilation alone
does not verify UAC or installer lifecycle behavior.

## Icons

The shared vector source is `../icons/source.svg`. Regenerate using the pinned
CLI, then retain only the desktop assets used by `tauri.conf.json`:

```sh
cargo tauri icon src-tauri/icons/source.svg --output dist/icon-generation
```

Copy `icon.png`, `icon.ico`, `icon.icns`, `32x32.png`, `128x128.png`, and
`128x128@2x.png` into `src-tauri/icons/`. The Windows app, setup executable,
uninstaller, and shortcuts use the ICO; macOS app bundles use the ICNS; Linux
desktop integration uses the PNGs. DMG files and Linux package files can show the
operating system's generic disk/package icon; their enclosed app carries DBsaur's
icon. AppImage file-manager previews depend on desktop integration support.
