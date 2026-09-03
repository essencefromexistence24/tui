use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-env-changed=GROK_VERSION");

    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let version = std::env::var("GROK_VERSION")
        .or_else(|_| std::env::var("CARGO_PKG_VERSION"))
        .unwrap_or_else(|_| "0.0.0".to_string());

    println!(
        "cargo:rustc-env=VERSION_WITH_COMMIT={} ({})",
        version, commit
    );
    embed_dx_figlet_fonts().expect("embed DX FIGlet fonts");
}

fn embed_dx_figlet_fonts() -> io::Result<()> {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let figlet_dir = manifest.join("../../common/dx/figlet");
    println!("cargo:rerun-if-changed={}", figlet_dir.display());
    let out_dir = PathBuf::from(
        std::env::var_os("OUT_DIR")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "OUT_DIR is not set"))?,
    );
    let mut fonts = Vec::new();
    for entry in fs::read_dir(&figlet_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) == Some("dx")
            && let Some(name) = path.file_stem().and_then(|value| value.to_str())
        {
            fonts.push((name.to_owned(), zstd::encode_all(&fs::read(path)?[..], 3)?));
        }
    }
    fonts.sort_by(|left, right| left.0.cmp(&right.0));
    let mut packed = Vec::new();
    packed.extend_from_slice(&(fonts.len() as u32).to_le_bytes());
    for (name, data) in fonts {
        packed.extend_from_slice(&(name.len() as u32).to_le_bytes());
        packed.extend_from_slice(name.as_bytes());
        packed.extend_from_slice(&(data.len() as u32).to_le_bytes());
        packed.extend_from_slice(&data);
    }
    let mut output = File::create(out_dir.join("fonts_data.rs"))?;
    writeln!(output, "pub const FONTS_DATA: &[u8] = &{:?};", packed)?;
    Ok(())
}


