use crate::Result;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug)]
enum Package {
    Windows,
    Debian,
    Rpm,
    Mac,
}

impl Package {
    fn bundle(self) -> &'static str {
        match self {
            Self::Windows => "nsis",
            Self::Debian => "deb",
            Self::Rpm => "rpm",
            Self::Mac => "app",
        }
    }

    fn directory(self) -> &'static str {
        match self {
            Self::Mac => "macos",
            _ => self.bundle(),
        }
    }

    fn matches(self, entry: &std::fs::DirEntry) -> std::io::Result<bool> {
        let kind = entry.file_type()?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        Ok(match self {
            Self::Windows => {
                kind.is_file() && name.starts_with("DBsaur_") && name.ends_with("-setup.exe")
            }
            Self::Debian => {
                kind.is_file()
                    && name.to_lowercase().starts_with("dbsaur_")
                    && name.ends_with(".deb")
            }
            Self::Rpm => {
                kind.is_file()
                    && name.to_lowercase().starts_with("dbsaur-")
                    && name.ends_with(".rpm")
            }
            Self::Mac => {
                kind.is_dir()
                    && name == "DBsaur.app"
                    && entry.path().join("Contents/MacOS/dbsaur-desktop").is_file()
            }
        })
    }
}

fn newest_installer(directory: &Path, package: Package) -> Result<PathBuf> {
    let mut installers = Vec::new();
    for entry in std::fs::read_dir(directory).map_err(|error| {
        format!(
            "No {} output at {} ({error}). Run cargo xtask build --bundles {} first.",
            package.bundle(),
            directory.display(),
            package.bundle()
        )
    })? {
        let entry = entry?;
        if package.matches(&entry)? {
            installers.push((entry.metadata()?.modified()?, entry.path()));
        }
    }
    installers.sort();
    installers.pop().map(|(_, path)| path).ok_or_else(|| {
        format!(
            "No DBsaur {} package found. Run cargo xtask build --bundles {} first.",
            package.bundle(),
            package.bundle()
        )
        .into()
    })
}

fn available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

pub fn install(root: &Path, args: &[String]) -> Result {
    let mut no_build = false;
    let mut dry_run = false;
    let mut universal = false;
    for arg in args {
        match arg.as_str() {
            "--no-build" => no_build = true,
            "--dry-run" => dry_run = true,
            "--universal" if cfg!(target_os = "macos") => universal = true,
            _ => return Err(format!("Unknown install option: {arg}").into()),
        }
    }
    let package = if cfg!(windows) {
        Package::Windows
    } else if cfg!(target_os = "macos") {
        Package::Mac
    } else if cfg!(target_os = "linux") {
        if available("apt-get") {
            Package::Debian
        } else if available("dnf") {
            Package::Rpm
        } else {
            return Err("Automatic Linux installation requires APT or DNF. Other distributions can use the AppImage from cargo xtask build --bundles appimage.".into());
        }
    } else {
        return Err("Desktop installation supports Windows, Linux, and macOS.".into());
    };
    let mut build_args = vec!["--bundles".into(), package.bundle().into()];
    if universal {
        build_args.extend(["--target".into(), "universal-apple-darwin".into()]);
    }
    if !no_build && !dry_run {
        crate::desktop(root, "build", &build_args)?;
    }
    let output = root.join(if universal {
        "dist/universal-apple-darwin/release/bundle"
    } else {
        "dist/release/bundle"
    });
    let installer = newest_installer(&output.join(package.directory()), package)?;
    if !installer
        .canonicalize()?
        .starts_with(root.join("dist").canonicalize()?)
    {
        return Err("Refusing to install a package outside the project's dist directory.".into());
    }
    println!("Installer: {}", installer.display());
    match package {
        Package::Windows => install_windows(&installer, dry_run),
        Package::Debian | Package::Rpm => {
            let manager = if matches!(package, Package::Debian) {
                "apt-get"
            } else {
                "dnf"
            };
            println!("Install using sudo {manager} install (system package manager).");
            if !dry_run {
                crate::run(
                    Command::new("sudo")
                        .args([manager, "install", "-y"])
                        .arg(&installer),
                )?;
            }
            Ok(())
        }
        Package::Mac => {
            println!("Destination: /Applications/DBsaur.app");
            if !dry_run {
                crate::run(
                    Command::new("sudo")
                        .args(["/usr/bin/ditto", "--rsrc", "--extattr"])
                        .arg(&installer)
                        .arg("/Applications/DBsaur.app"),
                )?;
            }
            Ok(())
        }
    }
}

fn install_windows(installer: &Path, dry_run: bool) -> Result {
    let program_files = std::env::var_os("ProgramW6432")
        .or_else(|| std::env::var_os("ProgramFiles"))
        .ok_or("Windows Program Files directory is unavailable")?;
    let destination = PathBuf::from(program_files).join("DBsaur");
    println!("Destination: {}", destination.display());
    if dry_run {
        println!("Dry run: would request UAC elevation and run setup silently; no changes made.");
        return Ok(());
    }
    // Only the installer is elevated. Paths are passed as environment data, never PowerShell code.
    // NSIS requires /D to be last and unquoted, even when the directory contains spaces.
    let script = r#"$ErrorActionPreference = 'Stop'
try {
    $arguments = '/S /D=' + $env:DBSAUR_INSTALL_DIR
    $process = Start-Process -FilePath $env:DBSAUR_INSTALLER_PATH -ArgumentList $arguments -Verb RunAs -WindowStyle Hidden -Wait -PassThru
    exit $process.ExitCode
} catch {
    Write-Error $_
    exit 1
}"#;
    crate::run(
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("DBSAUR_INSTALLER_PATH", installer)
            .env("DBSAUR_INSTALL_DIR", &destination),
    )?;
    if !destination.join("dbsaur-desktop.exe").is_file() {
        return Err(
            "Setup exited without creating the expected executable in Program Files.".into(),
        );
    }
    println!("Installed DBsaur in {}", destination.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn installer_selection_rejects_missing_output_and_unrelated_executables() {
        let directory = std::env::temp_dir().join(format!(
            "dbsaur-xtask-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(newest_installer(&directory, Package::Windows).is_err());
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("other.exe"), []).unwrap();
        fs::create_dir(directory.join("DBsaur_fake-setup.exe")).unwrap();
        assert!(newest_installer(&directory, Package::Windows).is_err());
        let installer = directory.join("DBsaur_0.1.0_x64-setup.exe");
        fs::write(&installer, []).unwrap();
        assert_eq!(
            newest_installer(&directory, Package::Windows).unwrap(),
            installer
        );
        let newer = directory.join("DBsaur_0.2.0_x64-setup.exe");
        fs::write(&newer, []).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&installer)
            .unwrap()
            .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1))
            .unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&newer)
            .unwrap()
            .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(
            newest_installer(&directory, Package::Windows).unwrap(),
            newer
        );
        assert!(newest_installer(&directory, Package::Debian).is_err());
        assert!(newest_installer(&directory, Package::Rpm).is_err());
        assert!(newest_installer(&directory, Package::Mac).is_err());
        let deb = directory.join("DBsaur_0.1.0_amd64.deb");
        fs::write(&deb, []).unwrap();
        assert_eq!(newest_installer(&directory, Package::Debian).unwrap(), deb);
        let rpm = directory.join("DBsaur-0.1.0-1.x86_64.rpm");
        fs::write(&rpm, []).unwrap();
        assert_eq!(newest_installer(&directory, Package::Rpm).unwrap(), rpm);
        let app = directory.join("DBsaur.app");
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        assert!(newest_installer(&directory, Package::Mac).is_err());
        fs::write(app.join("Contents/MacOS/dbsaur-desktop"), []).unwrap();
        assert_eq!(newest_installer(&directory, Package::Mac).unwrap(), app);
        // This directory was created uniquely by this test and contains only its fixtures.
        let resolved = directory.canonicalize().unwrap();
        assert_eq!(
            resolved.parent().unwrap(),
            std::env::temp_dir().canonicalize().unwrap()
        );
        fs::remove_dir_all(resolved).unwrap();
    }
}
