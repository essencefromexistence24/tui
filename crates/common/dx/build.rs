//! Build script to embed figlet fonts at compile time

use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

fn main() -> io::Result<()> {
	println!("cargo:rerun-if-changed=figlet");

	let out_dir = env::var_os("OUT_DIR")
		.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "OUT_DIR is not set"))?;
	let dest_path = PathBuf::from(out_dir).join("fonts_data.rs");
	let mut output = File::create(dest_path)?;

	// Get all .dx font files from figlet directory
	let figlet_dir = Path::new("figlet");

	if !figlet_dir.exists() {
		eprintln!("Warning: figlet directory not found at ./figlet");
		writeln!(output, "pub const FONTS_DATA: &[u8] = &[];")?;
		return Ok(());
	}

	let mut fonts = Vec::new();

	for entry in fs::read_dir(figlet_dir)? {
		let entry = entry?;
		let path = entry.path();

		if path.extension().and_then(|s| s.to_str()) == Some("dx")
			&& let Some(name) = path.file_stem().and_then(|s| s.to_str())
		{
			let data = fs::read(&path)?;

			// Compress with zstd
			let compressed = zstd::encode_all(&data[..], 3)?;

			fonts.push((name.to_string(), compressed));
		}
	}

	// Sort fonts by name for consistency
	fonts.sort_by(|a, b| a.0.cmp(&b.0));

	// Build binary format:
	// [u32: count] [name_len: u32][name: bytes][data_len: u32][data: bytes] ...
	let mut binary_data = Vec::new();

	// Write count
	binary_data.extend_from_slice(&(fonts.len() as u32).to_le_bytes());

	// Write each font
	for (name, data) in &fonts {
		let name_bytes = name.as_bytes();
		binary_data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
		binary_data.extend_from_slice(name_bytes);
		binary_data.extend_from_slice(&(data.len() as u32).to_le_bytes());
		binary_data.extend_from_slice(data);
	}

	// Write as Rust byte array
	writeln!(output, "pub const FONTS_DATA: &[u8] = &[")?;
	for (i, byte) in binary_data.iter().enumerate() {
		if i % 16 == 0 {
			write!(output, "    ")?;
		}
		write!(output, "0x{:02x},", byte)?;
		if i % 16 == 15 {
			writeln!(output)?;
		} else {
			write!(output, " ")?;
		}
	}
	if binary_data.len() % 16 != 0 {
		writeln!(output)?;
	}
	writeln!(output, "];")?;

	// Silenced: println!("cargo:warning=Embedded {} figlet fonts ({} bytes compressed)", fonts.len(), binary_data.len());

	Ok(())
}
