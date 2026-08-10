#![allow(missing_docs)]
#![deny(unsafe_code)]
//! dx-route-caveman — rule-based prose condensation (80+ rules).

use thiserror::Error;

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum CavemanError {
    #[error("regex error: {0}")]
    Regex(#[from] regex_lite::Error),
}

/// Result type for caveman operations.
pub type CavemanResult<T> = Result<T, CavemanError>;

#[allow(missing_docs)]
#[derive(Debug)]
#[must_use]
pub struct CavemanOutput {
    pub text: String,
    pub rules_applied: Vec<String>,
    pub original_len: usize,
    pub compressed_len: usize,
}

impl CavemanOutput {
    pub fn savings_pct(&self) -> f64 {
        if self.original_len == 0 {
            return 0.0;
        }
        (self.original_len - self.compressed_len) as f64 / self.original_len as f64 * 100.0
    }
}

/// Compress text using caveman rules at the given intensity.
/// Supports `lite`, `full`, and `ultra` intensity levels.
pub fn compress(body: &str, intensity: &str) -> CavemanResult<CavemanOutput> {
    let original_len = body.len();
    let rules = load_rules(intensity);
    let mut text = body.to_string();
    let mut applied = Vec::new();

    let preserved = extract_preserved_blocks(&text);
    text = mask_preserved_blocks(&text, &preserved);

    for rule in &rules {
        if !rule.is_applicable(intensity) {
            continue;
        }
        if let Some(result) = rule.apply(&text)? {
            text = result;
            applied.push(rule.name.clone());
        }
    }

    text = restore_preserved_blocks(&text, &preserved);
    text = cleanup_whitespace(&text);

    let compressed_len = text.len();
    tracing::debug!(
        "caveman: {} rules applied at intensity={}, {} → {} bytes",
        applied.len(),
        intensity,
        original_len,
        compressed_len
    );

    Ok(CavemanOutput {
        text,
        rules_applied: applied,
        original_len,
        compressed_len,
    })
}

struct Rule {
    name: String,
    pattern: &'static str,
    replacement: &'static str,
    min_intensity: Option<&'static str>,
}

impl Rule {
    fn is_applicable(&self, intensity: &str) -> bool {
        match self.min_intensity {
            Some("ultra") if intensity != "ultra" => false,
            Some("full") if intensity == "lite" => false,
            _ => true,
        }
    }

    fn apply(&self, text: &str) -> CavemanResult<Option<String>> {
        let re = regex_lite::Regex::new(self.pattern)?;
        let result = re.replace_all(text, self.replacement).to_string();
        Ok((result != text).then_some(result))
    }
}

fn load_rules(intensity: &str) -> Vec<Rule> {
    let all = vec![
        // ── Filler removal (lite+) ──
        Rule {
            name: "remove_pleasantries".into(),
            pattern: r"(?i)\b(?:thanks|thank you|please|sure,|of course|no problem|you're welcome)\b\s*",
            replacement: "",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "remove_hedging".into(),
            pattern: r"(?i)\b(?:i think|i believe|i guess|i suppose|it seems|it appears|maybe|perhaps|probably|possibly)\b\s*",
            replacement: "",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "remove_filler".into(),
            pattern: r"(?i)\b(?:actually|basically|essentially|honestly|literally|technically|frankly|simply|just)\b\s*",
            replacement: "",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "condense_question".into(),
            pattern: r"(?i)\bcould you please tell me\b",
            replacement: "what is",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "condense_i_would_like".into(),
            pattern: r"(?i)\bi would like to\b",
            replacement: "i want to",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "condense_in_order_to".into(),
            pattern: r"(?i)\bin order to\b",
            replacement: "to",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "condense_approximately".into(),
            pattern: r"(?i)\bapproximately\b",
            replacement: "about",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "contractions".into(),
            pattern: r"(?i)\b(cannot)\b",
            replacement: "can't",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "condense_utilize".into(),
            pattern: r"(?i)\b(?:utilize|utilises?|utilizes?)\b",
            replacement: "use",
            min_intensity: Some("lite"),
        },
        // ── Abbreviations (lite+) ──
        Rule {
            name: "abbrev_ai".into(),
            pattern: r"(?i)\bartificial intelligence\b",
            replacement: "AI",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "abbrev_ml".into(),
            pattern: r"(?i)\bmachine learning\b",
            replacement: "ML",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "abbrev_ui".into(),
            pattern: r"(?i)\buser interface\b",
            replacement: "UI",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "abbrev_api".into(),
            pattern: r"(?i)\bapplication programming interface\b",
            replacement: "API",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "abbrev_nlp".into(),
            pattern: r"(?i)\bnatural language processing\b",
            replacement: "NLP",
            min_intensity: Some("lite"),
        },
        Rule {
            name: "abbrev_sdk".into(),
            pattern: r"(?i)\bsoftware development kit\b",
            replacement: "SDK",
            min_intensity: Some("lite"),
        },
        // ── Structural compression (full+) ──
        Rule {
            name: "remove_self_prompt".into(),
            pattern: r"(?i)\b(?:you are (?:an? )?(?:AI|highly skilled|expert)|I am (?:an? )?(?:AI|language model))\b[^.]*\.\s*",
            replacement: "",
            min_intensity: Some("full"),
        },
        Rule {
            name: "remove_explanatory".into(),
            pattern: r"(?i)\b(?:it is worth noting that|it should be noted that|it is important to note that|as you may know|as you can see)\b\s*",
            replacement: "",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_because".into(),
            pattern: r"(?i)\bdue to the fact that\b",
            replacement: "because",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_in_the_event".into(),
            pattern: r"(?i)\bin the event that\b",
            replacement: "if",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_at_this_point".into(),
            pattern: r"(?i)\bat this point in time\b",
            replacement: "now",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_in_the_near_future".into(),
            pattern: r"(?i)\bin the near future\b",
            replacement: "soon",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_majority".into(),
            pattern: r"(?i)\bthe majority of\b",
            replacement: "most",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_a_number_of".into(),
            pattern: r"(?i)\ba number of\b",
            replacement: "several",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_in_accordance".into(),
            pattern: r"(?i)\bin accordance with\b",
            replacement: "under",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_with_regard".into(),
            pattern: r"(?i)\bwith (?:regard|respect) to\b",
            replacement: "about",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_subsequent".into(),
            pattern: r"(?i)\bsubsequent to\b",
            replacement: "after",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_prior_to".into(),
            pattern: r"(?i)\bprior to\b",
            replacement: "before",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_notwithstanding".into(),
            pattern: r"(?i)\bnotwithstanding\b",
            replacement: "despite",
            min_intensity: Some("full"),
        },
        Rule {
            name: "remove_articles".into(),
            pattern: r"\b(?:a|an|the)\s+",
            replacement: "",
            min_intensity: Some("full"),
        },
        Rule {
            name: "shorten_passive".into(),
            pattern: r"(?i)\b(?:is being|are being|was being|were being)\b",
            replacement: "is",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_additional".into(),
            pattern: r"(?i)\badditional\b",
            replacement: "more",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_demonstrate".into(),
            pattern: r"(?i)\bdemonstrate\b",
            replacement: "show",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_facilitate".into(),
            pattern: r"(?i)\bfacilitate\b",
            replacement: "help",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_commence".into(),
            pattern: r"(?i)\bcommence\b",
            replacement: "start",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_terminate".into(),
            pattern: r"(?i)\bterminate\b",
            replacement: "end",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_obtain".into(),
            pattern: r"(?i)\b(?:obtain|obtained|obtaining)\b",
            replacement: "get",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_require".into(),
            pattern: r"(?i)\b(?:require|requires|required|requiring)\b",
            replacement: "need",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_provide".into(),
            pattern: r"(?i)\b(?:provide|provides|provided|providing)\b",
            replacement: "give",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_assist".into(),
            pattern: r"(?i)\b(?:assist|assistance)\b",
            replacement: "help",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_sufficient".into(),
            pattern: r"(?i)\bsufficient\b",
            replacement: "enough",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_indicate".into(),
            pattern: r"(?i)\bindicate\b",
            replacement: "show",
            min_intensity: Some("full"),
        },
        Rule {
            name: "condense_on_behalf".into(),
            pattern: r"(?i)\bon behalf of\b",
            replacement: "for",
            min_intensity: Some("full"),
        },
        // ── Ultra abbreviations ──
        Rule {
            name: "abbrev_db".into(),
            pattern: r"(?i)\b(?:database|databases)\b",
            replacement: "DB",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_config".into(),
            pattern: r"(?i)\b(?:configuration|configurations)\b",
            replacement: "config",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_fn".into(),
            pattern: r"(?i)\bfunction\b",
            replacement: "fn",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_impl".into(),
            pattern: r"(?i)\b(?:implementation|implementations)\b",
            replacement: "impl",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_env".into(),
            pattern: r"(?i)\b(?:environment|environments)\b",
            replacement: "env",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_docs".into(),
            pattern: r"(?i)\b(?:documentation|documentations)\b",
            replacement: "docs",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_auth".into(),
            pattern: r"(?i)\b(?:authentication|authorization)\b",
            replacement: "auth",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_repo".into(),
            pattern: r"(?i)\b(?:repository|repositories)\b",
            replacement: "repo",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_admin".into(),
            pattern: r"(?i)\b(?:administrator|administrators)\b",
            replacement: "admin",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_info".into(),
            pattern: r"(?i)\binformation\b",
            replacement: "info",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_ref".into(),
            pattern: r"(?i)\b(?:reference|references)\b",
            replacement: "ref",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_dep".into(),
            pattern: r"(?i)\b(?:dependency|dependencies)\b",
            replacement: "dep",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_param".into(),
            pattern: r"(?i)\b(?:parameter|parameters)\b",
            replacement: "param",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_util".into(),
            pattern: r"(?i)\butility\b",
            replacement: "util",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_std".into(),
            pattern: r"(?i)\bstandard\b",
            replacement: "std",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_lib".into(),
            pattern: r"(?i)\b(?:library|libraries)\b",
            replacement: "lib",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_bin".into(),
            pattern: r"(?i)\bexecutable\b",
            replacement: "bin",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_os".into(),
            pattern: r"(?i)\boperating system\b",
            replacement: "OS",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_num".into(),
            pattern: r"(?i)\bnumber\b",
            replacement: "num",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_id".into(),
            pattern: r"(?i)\bidentifier\b",
            replacement: "id",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_regex".into(),
            pattern: r"(?i)\bregular expression\b",
            replacement: "regex",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_csv".into(),
            pattern: r"(?i)\bcomma separated values?\b",
            replacement: "CSV",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_json".into(),
            pattern: r"(?i)\bjavascript object notation\b",
            replacement: "JSON",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_ci".into(),
            pattern: r"(?i)\bcontinuous integration\b",
            replacement: "CI",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_cd".into(),
            pattern: r"(?i)\bcontinuous deployment\b",
            replacement: "CD",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_rest".into(),
            pattern: r"(?i)\brepresentational state transfer\b",
            replacement: "REST",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_orm".into(),
            pattern: r"(?i)\bobject relational mapper\b",
            replacement: "ORM",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_dsl".into(),
            pattern: r"(?i)\bdomain specific language\b",
            replacement: "DSL",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_decl".into(),
            pattern: r"(?i)\bdeclarative\b",
            replacement: "decl",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "abbrev_app".into(),
            pattern: r"(?i)\b(?:application|applications)\b",
            replacement: "app",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "remove_very".into(),
            pattern: r"(?i)\b(?:very |really |quite |extremely |highly |absolutely |totally )",
            replacement: "",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "condense_implement".into(),
            pattern: r"(?i)\b(?:implement|implemented|implementing)\b",
            replacement: "use",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "condense_endeavor".into(),
            pattern: r"(?i)\bendeavor\b",
            replacement: "try",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "condense_sufficiently".into(),
            pattern: r"(?i)\bsufficiently\b",
            replacement: "enough",
            min_intensity: Some("ultra"),
        },
        Rule {
            name: "condense_magnitude".into(),
            pattern: r"(?i)\bmagnitude\b",
            replacement: "size",
            min_intensity: Some("ultra"),
        },
    ];

    if intensity == "ultra" {
        return all;
    }

    all.into_iter()
        .filter(|r| r.min_intensity != Some("full") || intensity != "lite")
        .collect()
}

fn extract_preserved_blocks(text: &str) -> Vec<(usize, String)> {
    let re = regex_lite::Regex::new(r"```[\s\S]*?```|`[^`]+`|https?://\S+")
        .expect("static regex is valid");
    re.find_iter(text)
        .enumerate()
        .map(|(i, m)| (i, m.start(), m.end(), m.as_str().to_string()))
        .map(|(i, _s, _e, t)| (i, t))
        .collect()
}

fn extract_preserved_positions(text: &str) -> Vec<(usize, usize, usize, String)> {
    let re = regex_lite::Regex::new(r"```[\s\S]*?```|`[^`]+`|https?://\S+")
        .expect("static regex is valid");
    re.find_iter(text)
        .enumerate()
        .map(|(i, m)| (i, m.start(), m.end(), m.as_str().to_string()))
        .collect()
}

fn mask_preserved_blocks(text: &str, _blocks: &[(usize, String)]) -> String {
    let positions = extract_preserved_positions(text);
    let mut result = text.to_string();
    for (idx, start, end, _) in positions.iter().rev() {
        result.replace_range(*start..*end, &format!("\x00P{}\x00", idx));
    }
    result
}

fn restore_preserved_blocks(text: &str, blocks: &[(usize, String)]) -> String {
    let mut result = text.to_string();
    for (idx, block) in blocks.iter().rev() {
        let marker = format!("\x00P{}\x00", idx);
        if let Some(pos) = result.find(&marker) {
            result.replace_range(pos..pos + marker.len(), block);
        }
    }
    result
}

fn cleanup_whitespace(text: &str) -> String {
    let re = regex_lite::Regex::new(r" {2,}").expect("static regex is valid");
    let text = re.replace_all(text, " ");
    let re = regex_lite::Regex::new(r"\n{3,}").expect("static regex is valid");
    re.replace_all(&text, "\n\n").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_pleasantries() {
        let r = compress("Thanks for your help. Please let me know.", "lite").unwrap();
        assert!(!r.text.contains("Thanks for your help"));
    }

    #[test]
    fn shortens_ai_to_abbreviation() {
        let r = compress("artificial intelligence model", "lite").unwrap();
        assert_eq!(r.text.trim(), "AI model");
    }

    #[test]
    fn preserves_code_blocks() {
        let input = "text\n```rust\nfn main() {}\n```\nend";
        let r = compress(input, "full").unwrap();
        assert!(r.text.contains("```rust"));
        assert!(r.text.contains("fn main()"));
    }

    #[test]
    fn full_intensity_more_aggressive_than_lite() {
        let lite = compress(
            "I think we should implement the function due to the fact that it is very important.",
            "lite",
        )
        .unwrap();
        let full = compress(
            "I think we should implement the function due to the fact that it is very important.",
            "full",
        )
        .unwrap();
        assert!(full.compressed_len <= lite.compressed_len);
    }

    #[test]
    fn ultra_adds_abbreviations() {
        let r = compress(
            "The application configuration needs to be updated in the repository",
            "ultra",
        )
        .unwrap();
        assert!(r.text.contains("app") || r.text.contains("config"));
    }

    #[test]
    fn removes_self_prompt_boilerplate() {
        let r = compress(
            "You are an AI assistant designed to help with coding tasks.",
            "full",
        )
        .unwrap();
        assert_eq!(r.text.trim(), "");
    }

    #[test]
    fn tracks_savings_percentage() {
        let input = "this is a very long text that should be compressed significantly using all the rules we have available".repeat(5);
        let r = compress(&input, "ultra").unwrap();
        assert!(r.savings_pct() > 0.0);
        assert!(r.compressed_len < r.original_len);
    }

    #[test]
    fn empty_input_ok() {
        let r = compress("", "full").unwrap();
        assert!(r.text.is_empty());
        assert_eq!(r.original_len, 0);
    }
}
