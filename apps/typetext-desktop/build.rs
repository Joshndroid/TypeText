use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../icon/TypeText.ico");

    if env::var("CARGO_CFG_WINDOWS").is_err() {
        return;
    }

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
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
