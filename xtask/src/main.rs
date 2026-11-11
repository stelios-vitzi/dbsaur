use std::{env, error::Error, path::Path, process::Command};

mod install;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn run(command: &mut Command) -> Result {
    let status = command.status()?;
    if !status.success() {
        return Err(format!("Command failed ({status}): {command:?}").into());
    }
    Ok(())
}

fn desktop(root: &Path, action: &str, args: &[String]) -> Result {
    run(Command::new("cargo")
        .args(["tauri", action])
        .args(args)
        .env("CARGO_TARGET_DIR", root.join("dist")))?;
    if action == "build" {
        println!("Desktop build output: {}", root.join("dist").display());
    }
    Ok(())
}

fn main() -> Result {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    env::set_current_dir(root)?;
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("frontend") => {
            if args.next().is_some() {
                return Err("frontend accepts no arguments".into());
            }
            // cmd is needed for npm.cmd on Windows; no user arguments enter the shell.
            let mut command = if cfg!(windows) {
                let mut command = Command::new("cmd");
                command.args(["/d", "/c", "npm run build"]);
                command
            } else {
                let mut command = Command::new("npm");
                command.args(["run", "build"]);
                command
            };
            run(command.current_dir(root.join("frontend")))?;
        }
        Some(action @ ("dev" | "build")) => desktop(root, action, &args.collect::<Vec<_>>())?,
        Some("install") => install::install(root, &args.collect::<Vec<_>>())?,
        Some("check") => {
            if args.next().is_some() {
                return Err("check accepts no arguments".into());
            }
            run(Command::new("cargo")
                .args([
                    "check",
                    "--manifest-path",
                    "src-tauri/Cargo.toml",
                    "--locked",
                ])
                .env("CARGO_TARGET_DIR", root.join("dist")))?;
        }
        None | Some("help" | "--help" | "-h") => {
            println!(
                "cargo xtask dev       Open the desktop app (static frontend)\ncargo xtask build     Build native installers under dist/\ncargo xtask install   Build and install for this OS (UAC/sudo)\n                     --no-build: use the newest existing package in dist/\n                     --dry-run: inspect the existing package without building or installing\n                     --universal: use universal Apple Silicon + Intel app (macOS)\ncargo xtask frontend  Generate desktop frontend assets\ncargo xtask check     Check the desktop Rust code\nExtra dev/build arguments are forwarded to cargo tauri."
            );
        }
        Some(action) => return Err(format!("Unknown xtask: {action}").into()),
    }
    Ok(())
}
