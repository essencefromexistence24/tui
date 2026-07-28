// MCP Servers submenu
pub fn get_submenu() -> Vec<(&'static str, &'static str)> {
	vec![
		("1. STDIO Servers", ""),
		("2. HTTP Servers", ""),
		("3. OAuth Credentials", ""),
		("4. OAuth Callback Port", ""),
		("5. Server Management", ""),
	]
}
