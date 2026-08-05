//! Font module for DX TUI rendering.
//! Fonts are embedded at build time from figlet directory

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::io;

// Include the generated fonts data from build script
include!(concat!(env!("OUT_DIR"), "/fonts_data.rs"));

// Parse the binary font data into a HashMap
static FONTS: Lazy<HashMap<String, &'static [u8]>> = Lazy::new(|| {
    let mut fonts = HashMap::new();
    let mut offset = 0;

    if FONTS_DATA.len() < 4 {
        return fonts;
    }

    let count =
        u32::from_le_bytes([FONTS_DATA[0], FONTS_DATA[1], FONTS_DATA[2], FONTS_DATA[3]]) as usize;
    offset += 4;

    for _ in 0..count {
        if offset + 4 > FONTS_DATA.len() {
            break;
        }

        let name_len = u32::from_le_bytes([
            FONTS_DATA[offset],
            FONTS_DATA[offset + 1],
            FONTS_DATA[offset + 2],
            FONTS_DATA[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + name_len > FONTS_DATA.len() {
            break;
        }

        let name = String::from_utf8_lossy(&FONTS_DATA[offset..offset + name_len]).to_string();
        offset += name_len;

        if offset + 4 > FONTS_DATA.len() {
            break;
        }

        let data_len = u32::from_le_bytes([
            FONTS_DATA[offset],
            FONTS_DATA[offset + 1],
            FONTS_DATA[offset + 2],
            FONTS_DATA[offset + 3],
        ]) as usize;
        offset += 4;

        if offset + data_len > FONTS_DATA.len() {
            break;
        }

        let data = &FONTS_DATA[offset..offset + data_len];
        offset += data_len;

        fonts.insert(name, data);
    }

    fonts
});

/// Reads and decompresses a font file.
pub fn read_font(name: &str) -> io::Result<Vec<u8>> {
    // Case-insensitive lookup (build pack keeps original stems).
    if let Some(compressed) = FONTS.get(name) {
        return zstd::decode_all(*compressed)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
    }
    let lower = name.to_ascii_lowercase();
    for (k, compressed) in FONTS.iter() {
        if k.to_ascii_lowercase() == lower {
            return zstd::decode_all(*compressed)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("Font '{name}' not found"),
    ))
}

/// Embedded font stems (for splash font cycling).
pub fn list_font_names() -> Vec<String> {
    let mut names: Vec<String> = FONTS.keys().cloned().collect();
    names.sort();
    names
}
