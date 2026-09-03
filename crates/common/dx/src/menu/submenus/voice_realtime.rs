// Voice / Realtime submenu
pub fn get_submenu() -> Vec<(&'static str, &'static str)> {
    vec![
        ("1. STT model", "Parakeet TDT 0.6B v3 (default)"),
        ("2. TTS model", "Kokoro INT8"),
        ("3. Listen + wave bars", "Ctrl+S"),
        ("4. Speak selection / last answer", "Ctrl+T"),
        ("5. Voice panel", "/voice"),
        ("6. Helper LLM", "SmolChat (SmolLM2 135M)"),
        ("7. Agent / coding LLM", "minicpm5-1b-tooluse"),
    ]
}
