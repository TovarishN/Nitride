//! Compiled directly by rustc with baseline CPU flags, never with PGO payload flags.
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn optimized_cpu() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        // Keep this list in sync with release/build.py's AVX2_FEATURES.
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("bmi1")
            && std::is_x86_feature_detected!("bmi2")
            && std::is_x86_feature_detected!("lzcnt")
            && std::is_x86_feature_detected!("popcnt")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

fn payload(folder: &Path, optimized: bool) -> PathBuf {
    let name = |variant| folder.join(format!("nitride-{variant}{}", std::env::consts::EXE_SUFFIX));
    let avx2 = name("avx2");
    if optimized && avx2.is_file() {
        avx2
    } else {
        name("baseline")
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?;
    let binary = payload(
        executable.parent().ok_or("missing executable directory")?,
        optimized_cpu(),
    );
    let mut command = Command::new(&binary);
    command.args(std::env::args_os().skip(1));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(format!("could not start {}: {}", binary.display(), command.exec()).into())
    }
    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .map_err(|e| format!("could not start {}: {e}", binary.display()))?;
        // Preserve the full Windows exit code (ExitCode only accepts u8).
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Nitride launcher: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baseline_is_always_selected_without_cpu_support() {
        let folder = std::env::temp_dir().join(format!("nitride-launcher-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let avx2 = folder.join(format!("nitride-avx2{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&avx2, "").unwrap();
        assert_eq!(
            payload(&folder, false),
            folder.join(format!("nitride-baseline{}", std::env::consts::EXE_SUFFIX))
        );
        assert_eq!(payload(&folder, true), avx2);
        std::fs::remove_file(avx2).unwrap();
        assert_eq!(payload(&folder, true), payload(&folder, false));
        std::fs::remove_dir(folder).unwrap();
    }
}
