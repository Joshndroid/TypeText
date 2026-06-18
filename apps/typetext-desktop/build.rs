use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../icon/TypeText.ico");
    println!("cargo:rerun-if-changed=../../VERSION");
    println!("cargo:rerun-if-env-changed=TYPETEXT_VERSION");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let version = resolved_version(&manifest_dir);
    println!("cargo:rustc-env=TYPETEXT_APP_VERSION={version}");

    if env::var("CARGO_CFG_WINDOWS").is_err() {
        return;
    }

    let icon_path = manifest_dir.join("../../icon/TypeText.ico");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let res_path = out_dir.join("typetext.res");
    let rc_path = out_dir.join("typetext.rc");
    let icon_path = icon_path.display().to_string().replace('\\', "/");
    fs::write(&rc_path, format!("1 ICON \"{icon_path}\"\n"))
        .expect("Could not write generated Windows resource file");

    let compiled = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        Command::new("rc")
            .args(["/nologo", "/fo"])
            .arg(&res_path)
            .arg(&rc_path)
            .status()
            .is_ok_and(|status| status.success())
    } else {
        Command::new("windres")
            .arg(&rc_path)
            .args(["-O", "coff", "-o"])
            .arg(&res_path)
            .status()
            .is_ok_and(|status| status.success())
    };

    if compiled {
        println!("cargo:rustc-link-arg={}", res_path.display());
    } else {
        println!("cargo:warning=Could not compile Windows icon resource; runtime icon will still be used.");
    }
}

fn resolved_version(manifest_dir: &std::path::Path) -> String {
    if let Ok(version) = env::var("TYPETEXT_VERSION") {
        let version = version.trim();
        if !version.is_empty() {
            return version.to_string();
        }
    }

    let root_dir = manifest_dir.join("../..");
    if let Ok(output) = Command::new("git")
        .args(["describe", "--tags", "--exact-match"])
        .current_dir(&root_dir)
        .output()
    {
        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !version.is_empty() {
                return version;
            }
        }
    }

    let version_path = root_dir.join("VERSION");
    if let Ok(version) = fs::read_to_string(version_path) {
        let version = version.trim();
        if !version.is_empty() {
            return version.to_string();
        }
    }

    env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string())
}
