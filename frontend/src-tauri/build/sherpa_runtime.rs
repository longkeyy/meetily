use bzip2::read::BzDecoder;
use sha2::{Digest, Sha256};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION: &str = "1.13.4";
const ARCHIVE_NAME: &str = "sherpa-onnx-v1.13.4-osx-universal2-shared-lib.tar.bz2";
const ARCHIVE_SHA256: &str = "67150b8d9d6506f81ff876c3eb21e509e9575ce954230668a56322e3d1d835e0";
const RUNTIME_LIBS: [&str; 2] = ["libsherpa-onnx-c-api.dylib", "libonnxruntime.1.27.0.dylib"];

pub fn prepare() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return Ok(());
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let runtime_dir = manifest_dir.join("binaries/sherpa-macos");
    let marker = runtime_dir.join(".runtime-version");
    println!("cargo:rerun-if-changed={}", marker.display());

    if !runtime_is_current(&runtime_dir, &marker)? {
        fs::create_dir_all(&runtime_dir)?;
        let archive_path = runtime_dir.join(ARCHIVE_NAME);
        if !archive_matches(&archive_path)? {
            download_archive(&archive_path)?;
        }
        extract_runtime(&archive_path, &runtime_dir)?;
        fs::write(marker, format!("{VERSION}\n{ARCHIVE_SHA256}\n"))?;
        println!("cargo:warning=Prepared sherpa-onnx macOS shared runtime with CoreML support");
    }

    ensure_runtime_is_signed(&runtime_dir)?;
    stage_runtime(&runtime_dir)?;
    Ok(())
}

fn runtime_is_current(runtime_dir: &Path, marker: &Path) -> Result<bool, Box<dyn Error>> {
    let expected = format!("{VERSION}\n{ARCHIVE_SHA256}\n");
    Ok(
        fs::read_to_string(marker).ok().as_deref() == Some(expected.as_str())
            && RUNTIME_LIBS
                .iter()
                .all(|name| runtime_dir.join(name).is_file()),
    )
}

fn archive_matches(path: &Path) -> Result<bool, Box<dyn Error>> {
    if !path.is_file() {
        return Ok(false);
    }
    Ok(sha256(path)? == ARCHIVE_SHA256)
}

fn download_archive(path: &Path) -> Result<(), Box<dyn Error>> {
    let url = format!(
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/v{VERSION}/{ARCHIVE_NAME}"
    );
    println!("cargo:warning=Downloading sherpa-onnx macOS runtime from {url}");
    let mut response = reqwest::blocking::Client::builder()
        .user_agent("meetily-build")
        .build()?
        .get(url)
        .send()?
        .error_for_status()?;
    let part_path = path.with_extension("part");
    let mut output = File::create(&part_path)?;
    io::copy(&mut response, &mut output)?;
    drop(output);
    if !archive_matches(&part_path)? {
        let _ = fs::remove_file(&part_path);
        return Err("downloaded sherpa-onnx runtime checksum mismatch".into());
    }
    fs::rename(part_path, path)?;
    Ok(())
}

fn extract_runtime(archive_path: &Path, runtime_dir: &Path) -> Result<(), Box<dyn Error>> {
    let decoder = BzDecoder::new(File::open(archive_path)?);
    let mut archive = tar::Archive::new(decoder);
    let mut extracted = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let Some(name) = entry
            .path()?
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if RUNTIME_LIBS.contains(&name.as_str()) {
            entry.unpack(runtime_dir.join(&name))?;
            extracted.push(name);
        }
    }
    if !RUNTIME_LIBS
        .iter()
        .all(|name| extracted.iter().any(|value| value == name))
    {
        return Err("sherpa-onnx archive is missing required macOS runtime libraries".into());
    }
    Ok(())
}

fn stage_runtime(runtime_dir: &Path) -> Result<(), Box<dyn Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let profile = env::var("PROFILE")?;
    let profile_dir = out_dir
        .ancestors()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(profile.as_str()))
        .ok_or_else(|| {
            format!(
                "unable to locate Cargo profile directory from {}",
                out_dir.display()
            )
        })?;
    let target_dir = profile_dir
        .parent()
        .ok_or("Cargo profile directory has no parent")?;

    for frameworks_dir in [
        target_dir.join("Frameworks"),
        profile_dir.join("Frameworks"),
    ] {
        fs::create_dir_all(&frameworks_dir)?;
        for name in RUNTIME_LIBS {
            copy_atomically(&runtime_dir.join(name), &frameworks_dir.join(name))?;
        }
    }
    Ok(())
}

fn ensure_runtime_is_signed(runtime_dir: &Path) -> Result<(), Box<dyn Error>> {
    for name in RUNTIME_LIBS {
        let path = runtime_dir.join(name);
        let valid = Command::new("codesign")
            .args(["--verify", "--strict"])
            .arg(&path)
            .status()?
            .success();
        if valid {
            continue;
        }

        let status = Command::new("codesign")
            .args(["--force", "--sign", "-", "--timestamp=none"])
            .arg(&path)
            .status()?;
        if !status.success() {
            return Err(format!("failed to ad-hoc sign {}", path.display()).into());
        }
    }
    Ok(())
}

fn copy_atomically(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    let part_path = destination.with_extension("part");
    let _ = fs::remove_file(&part_path);
    fs::copy(source, &part_path)?;
    fs::rename(part_path, destination)?;
    Ok(())
}

fn sha256(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
