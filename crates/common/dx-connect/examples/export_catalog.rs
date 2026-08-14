use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let local = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
    } else if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        Some(path.into())
    } else {
        std::env::var_os("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".local").join("share"))
    }
    .ok_or("local data directory is unavailable")?;
    let directory = local.join("dx").join("connects");
    fs::create_dir_all(&directory)?;
    let path = directory.join("catalog.json");
    fs::write(
        path,
        serde_json::to_vec_pretty(&dx_connect::catalog_json())?,
    )?;
    println!("DX Connect catalog exported to {}", directory.display());
    Ok(())
}
