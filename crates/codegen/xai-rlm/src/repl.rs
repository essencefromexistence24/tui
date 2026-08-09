use crate::error::{RLMError, Result};
use rhai::{AST, Engine, Scope};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Sandboxed Rhai executor used by the RLM loop.
///
/// The executor intentionally exposes only a small set of fast string-search
/// primitives and bounded script execution so host applications can embed this
/// crate without spawning an external interpreter.
pub struct REPLExecutor {
    engine: Engine,
    max_output_chars: usize,
    ast_cache: Arc<Mutex<HashMap<String, AST>>>,
    cache_hits: Arc<Mutex<usize>>,
    cache_misses: Arc<Mutex<usize>>,
}

impl REPLExecutor {
    pub fn new() -> Self {
        let mut engine = Engine::new();

        engine.set_max_expr_depths(50, 50);
        engine.set_max_operations(100_000);
        engine.set_max_string_size(10_000_000);
        engine.set_max_array_size(20_000);
        engine.set_max_map_size(2_000);

        Self::register_fast_search(&mut engine);

        Self {
            engine,
            max_output_chars: 4_000,
            ast_cache: Arc::new(Mutex::new(HashMap::new())),
            cache_hits: Arc::new(Mutex::new(0)),
            cache_misses: Arc::new(Mutex::new(0)),
        }
    }

    pub fn cache_stats(&self) -> (usize, usize) {
        let hits = *self.cache_hits.lock().unwrap();
        let misses = *self.cache_misses.lock().unwrap();
        (hits, misses)
    }

    pub fn clear_cache(&self) {
        self.ast_cache.lock().unwrap().clear();
        *self.cache_hits.lock().unwrap() = 0;
        *self.cache_misses.lock().unwrap() = 0;
    }

    fn register_fast_search(engine: &mut Engine) {
        engine.register_fn("fast_find", |text: &str, pattern: &str| -> i64 {
            memchr::memmem::find(text.as_bytes(), pattern.as_bytes())
                .map(|index| index as i64)
                .unwrap_or(-1)
        });

        engine.register_fn("fast_rfind", |text: &str, pattern: &str| -> i64 {
            memchr::memmem::rfind(text.as_bytes(), pattern.as_bytes())
                .map(|index| index as i64)
                .unwrap_or(-1)
        });

        engine.register_fn("fast_contains", |text: &str, pattern: &str| -> bool {
            memchr::memmem::find(text.as_bytes(), pattern.as_bytes()).is_some()
        });

        engine.register_fn("fast_find_all", |text: &str, pattern: &str| -> Vec<i64> {
            memchr::memmem::find_iter(text.as_bytes(), pattern.as_bytes())
                .map(|index| index as i64)
                .collect()
        });

        engine.register_fn("fast_count", |text: &str, pattern: &str| -> i64 {
            memchr::memmem::find_iter(text.as_bytes(), pattern.as_bytes()).count() as i64
        });

        engine.register_fn("window", |text: &str, start: i64, len: i64| -> String {
            if start < 0 || len <= 0 {
                return String::new();
            }

            let start = start as usize;
            if start >= text.len() {
                return String::new();
            }

            let end = start.saturating_add(len as usize).min(text.len());
            text[start..end].to_string()
        });

        engine.register_fn("head", |text: &str, len: i64| -> String {
            if len <= 0 {
                return String::new();
            }

            let end = (len as usize).min(text.len());
            text[..end].to_string()
        });

        engine.register_fn("tail", |text: &str, len: i64| -> String {
            if len <= 0 {
                return String::new();
            }

            let len = len as usize;
            if len >= text.len() {
                return text.to_string();
            }

            text[text.len() - len..].to_string()
        });
    }

    pub fn execute(&self, code: &str, scope: &mut Scope) -> Result<String> {
        let code = self.extract_code(code);

        if code.trim().is_empty() {
            return Ok("No code to execute".to_string());
        }

        let ast = {
            let mut cache = self.ast_cache.lock().unwrap();

            if let Some(cached_ast) = cache.get(&code) {
                *self.cache_hits.lock().unwrap() += 1;
                cached_ast.clone()
            } else {
                *self.cache_misses.lock().unwrap() += 1;

                let ast = self
                    .engine
                    .compile(&code)
                    .map_err(|err| RLMError::REPLError(format!("Compilation error: {err}")))?;

                if cache.len() < 1_000 {
                    cache.insert(code.clone(), ast.clone());
                }

                ast
            }
        };

        let result: rhai::Dynamic = self
            .engine
            .eval_ast_with_scope(scope, &ast)
            .map_err(|err| RLMError::REPLError(format!("Execution error: {err}")))?;

        let output = result.to_string();

        if output.len() > self.max_output_chars {
            Ok(format!(
                "{}\n\n[Output truncated: {} chars total, showing first {}]",
                &output[..self.max_output_chars],
                output.len(),
                self.max_output_chars
            ))
        } else if output.is_empty() {
            Ok("Code executed successfully (no output)".to_string())
        } else {
            Ok(output)
        }
    }

    fn extract_code(&self, text: &str) -> String {
        if text.contains("```") {
            if let Some(start) = text.find("```rhai") {
                let start = start + 7;
                if let Some(end) = text[start..].find("```") {
                    return text[start..start + end].trim().to_string();
                }
            }

            if let Some(start) = text.find("```") {
                let start = start + 3;
                if let Some(end) = text[start..].find("```") {
                    return text[start..start + end].trim().to_string();
                }
            }
        }

        text.trim().to_string()
    }
}

impl Default for REPLExecutor {
    fn default() -> Self {
        Self::new()
    }
}
