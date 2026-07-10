use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../icon/TypeText.ico");
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let version = resolved_version();
    println!("cargo:rustc-env=TYPETEXT_APP_VERSION={version}");

    if env::var("CARGO_CFG_WINDOWS").is_err() {
        return;
    }

    let icon_path = fs::canonicalize(manifest_dir.join("../../icon/TypeText.ico"))
        .expect("Windows icon resource exists");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let res_path = out_dir.join("typetext.res");
    let rc_path = out_dir.join("typetext.rc");
    let icon_path = icon_path.display().to_string().replace('\\', "/");
    let display_version = version.strip_prefix('v').unwrap_or(&version);
    let (major, minor, patch, build) = windows_version_parts(display_version);
    let offline_portable = env::var_os("CARGO_FEATURE_OFFLINE_PORTABLE").is_some();
    let product_name = if offline_portable {
        "TypeText Offline Portable"
    } else {
        "TypeText"
    };
    let file_description = if offline_portable {
        "TypeText offline portable desktop application"
    } else {
        "TypeText desktop application"
    };
    let manifest_path = out_dir.join("typetext.manifest");
    let manifest = MANIFEST_TEMPLATE.replace("@VERSION@", &format!("{major}.{minor}.{patch}.{build}"));
    fs::write(&manifest_path, manifest).expect("Could not write generated application manifest");
    let manifest_path = manifest_path.display().to_string().replace('\\', "/");

    let resource = format!(
        r#"1 ICON "{icon_path}"

1 24 "{manifest_path}"

1 VERSIONINFO
 FILEVERSION {major},{minor},{patch},{build}
 PRODUCTVERSION {major},{minor},{patch},{build}
 FILEFLAGSMASK 0x3fL
 FILEFLAGS 0x0L
 FILEOS 0x40004L
 FILETYPE 0x1L
 FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "Joshndroid"
            VALUE "FileDescription", "{file_description}"
            VALUE "FileVersion", "{display_version}"
            VALUE "InternalName", "TypeText"
            VALUE "LegalCopyright", "Copyright (c) 2026 Joshndroid"
            VALUE "OriginalFilename", "TypeText.exe"
            VALUE "ProductName", "{product_name}"
            VALUE "ProductVersion", "{display_version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );
    fs::write(&rc_path, resource).expect("Could not write generated Windows resource file");

    let compiled = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        compile_msvc_resource(&res_path, &rc_path)
    } else {
        compile_gnu_resource(&res_path, &rc_path)
    };

    match compiled {
        Ok(ResourceCompile::Compiled) => {
            println!("cargo:rustc-link-arg={}", res_path.display());
        }
        Ok(ResourceCompile::Skipped) => {}
        Err(error) => println!(
            "cargo:warning=Could not compile Windows icon resource ({error}); runtime icon will still be used."
        ),
    }
}

/// Application manifest embedded as RT_MANIFEST (resource type 24, ID 1).
/// Declares asInvoker execution level (no elevation), supported Windows
/// versions, per-monitor-v2 DPI awareness, and long-path awareness.
const MANIFEST_TEMPLATE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="Joshndroid.TypeText" version="@VERSION@" processorArchitecture="*"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 and 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
      <!-- Windows 8.1 -->
      <supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}"/>
      <!-- Windows 8 -->
      <supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}"/>
      <!-- Windows 7 -->
      <supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
</assembly>
"#;

enum ResourceCompile {
    Compiled,
    Skipped,
}

fn compile_msvc_resource(
    res_path: &std::path::Path,
    rc_path: &std::path::Path,
) -> Result<ResourceCompile, String> {
    for rc_exe in msvc_rc_candidates() {
        match Command::new(&rc_exe)
            .args(["/nologo", "/fo"])
            .arg(res_path)
            .arg(rc_path)
            .output()
        {
            Ok(output) if output.status.success() => return Ok(ResourceCompile::Compiled),
            Ok(output) => {
                return Err(format!(
                    "{} exited with {}: {}{}",
                    PathBuf::from(&rc_exe).display(),
                    output.status,
                    String::from_utf8_lossy(&output.stdout).trim(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("{}: {error}", PathBuf::from(&rc_exe).display())),
        }
    }

    Ok(ResourceCompile::Skipped)
}

fn compile_gnu_resource(
    res_path: &std::path::Path,
    rc_path: &std::path::Path,
) -> Result<ResourceCompile, String> {
    let output = match Command::new("windres")
        .arg(rc_path)
        .args(["-O", "coff", "-o"])
        .arg(res_path)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ResourceCompile::Skipped);
        }
        Err(error) => return Err(format!("windres: {error}")),
    };

    if output.status.success() {
        Ok(ResourceCompile::Compiled)
    } else {
        Err(format!(
            "windres exited with {}: {}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn msvc_rc_candidates() -> Vec<OsString> {
    let mut candidates = vec![OsString::from("rc")];
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let sdk_arch = match target_arch.as_str() {
        "aarch64" => "arm64",
        "x86" => "x86",
        _ => "x64",
    };

    if let Some(path) = env::var_os("WindowsSdkBinPath") {
        candidates.push(PathBuf::from(path).join("rc.exe").into_os_string());
    }

    for root in windows_kit_roots() {
        let bin_dir = root.join("bin");
        let Ok(entries) = fs::read_dir(&bin_dir) else {
            continue;
        };

        let mut version_dirs = entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                if path.is_dir() { Some(path) } else { None }
            })
            .collect::<Vec<_>>();
        version_dirs.sort();

        for version_dir in version_dirs.into_iter().rev() {
            candidates.push(version_dir.join(sdk_arch).join("rc.exe").into_os_string());
            candidates.push(version_dir.join("x64").join("rc.exe").into_os_string());
        }
    }

    candidates
}

fn windows_kit_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(root) = env::var_os("WindowsSdkDir") {
        roots.push(PathBuf::from(root));
    }

    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(program_files) = env::var_os(var) {
            roots.push(PathBuf::from(program_files).join("Windows Kits").join("10"));
        }
    }

    roots
}

fn resolved_version() -> String {
    format!(
        "v{}",
        env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_string())
    )
}

fn windows_version_parts(version: &str) -> (u16, u16, u16, u16) {
    let mut parts = version
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>()
                .parse::<u16>()
                .unwrap_or(0)
        })
        .chain(std::iter::repeat(0));

    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}
