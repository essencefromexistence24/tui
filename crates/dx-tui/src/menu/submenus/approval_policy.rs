// Approval Policy submenu
pub fn get_submenu() -> Vec<(&'static str, &'static str)> {
	vec![
		("1. Policy Mode", ""),
		("2. Untrusted Mode", ""),
		("3. On-Request Mode", ""),
		("4. Never Mode", ""),
		("5. Granular Permissions", ""),
	]
}
