// Providers submenu — production shortcuts into live pickers
pub fn get_submenu() -> Vec<(&'static str, &'static str)> {
    vec![
        ("1. Model menu (key 0)", "Flow · Zen · catalog"),
        ("2. Connect provider", "/providers · models.dev"),
        ("3. Local runtime", "dx-flow GGUF"),
        ("4. Remote runtime", "cloud providers"),
        ("5. Refresh models.dev", "catalog cache"),
        ("6. Provider doctor", "/status providers"),
    ]
}
