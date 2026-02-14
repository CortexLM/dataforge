//! Candidate filtering heuristics for SWE PR mining.

#[derive(Debug, Clone)]
pub struct FilterConfig {
    pub min_stars: u32,
    pub min_files: usize,
    pub max_files: usize,
    pub min_added_lines: usize,
    pub max_added_lines: usize,
    pub allowed_languages: Vec<String>,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            min_stars: 20,
            min_files: 1,
            max_files: 50,
            min_added_lines: 3,
            max_added_lines: 1000,
            allowed_languages: vec![
                "python".to_string(),
                "javascript".to_string(),
                "typescript".to_string(),
                "go".to_string(),
                "rust".to_string(),
                "java".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct FilterResult {
    pub accepted: bool,
    pub score: f64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SweepFilter {
    config: FilterConfig,
}

impl SweepFilter {
    pub fn new(config: FilterConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(FilterConfig::default())
    }

    pub fn keep_candidate(
        &self,
        language: &str,
        stars: u32,
        files_changed: usize,
        _added_lines: usize,
    ) -> FilterResult {
        let mut reasons = Vec::new();
        let mut score = 1.0f64;
        let normalized_language = language.to_lowercase();

        let lang_unknown = normalized_language.is_empty()
            || normalized_language == "unknown"
            || normalized_language == "null";

        // Only reject known languages not in whitelist; unknown languages pass
        if !self.config.allowed_languages.is_empty()
            && !lang_unknown
            && !self.config
                .allowed_languages
                .iter()
                .any(|l| l.eq_ignore_ascii_case(&normalized_language))
        {
            reasons.push(format!("language '{language}' not in whitelist"));
            score -= 0.4;
        }

        // Only apply stars filter if stars > 0 (i.e. data is available)
        if stars > 0 && stars < self.config.min_stars {
            reasons.push(format!("stars {} below minimum {}", stars, self.config.min_stars));
            score -= 0.3;
        }

        // Skip files/lines checks when data is not reliable (enrichment may have failed)
        if files_changed > self.config.max_files {
            reasons.push(format!(
                "files changed {files_changed} above max {}",
                self.config.max_files
            ));
            score -= 0.25;
        }

        let accepted = reasons.is_empty();
        if accepted {
            reasons.push("candidate accepted".to_string());
        }

        FilterResult {
            accepted,
            score: score.clamp(0.0, 1.0),
            reasons,
        }
    }
}
