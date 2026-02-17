//! Candidate filtering heuristics for SWE PR mining.

#[derive(Debug, Clone)]
pub struct FilterConfig {
    pub min_stars: u32,
    pub min_files: usize,
    pub max_files: usize,
    pub min_added_lines: usize,
    pub max_added_lines: usize,
    pub allowed_languages: Vec<String>,
    /// Minimum combined length of PR title + body (in characters) to accept a candidate.
    /// PRs with empty or very short descriptions are unlikely to produce good benchmark tasks.
    pub min_description_length: usize,
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
            min_description_length: 30,
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

    /// Evaluate whether a PR candidate should be kept for further processing.
    ///
    /// # Arguments
    ///
    /// * `language` - Primary language of the repository
    /// * `stars` - Repository star count (0 means unknown)
    /// * `files_changed` - Number of files changed in the PR
    /// * `added_lines` - Number of lines added
    /// * `changed_files` - List of changed file paths
    /// * `title` - PR title
    /// * `body` - PR body/description
    #[allow(clippy::too_many_arguments)]
    pub fn keep_candidate(
        &self,
        language: &str,
        stars: u32,
        files_changed: usize,
        added_lines: usize,
        changed_files: &[String],
        title: &str,
        body: &str,
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
            && !self
                .config
                .allowed_languages
                .iter()
                .any(|l| l.eq_ignore_ascii_case(&normalized_language))
        {
            reasons.push(format!("language '{language}' not in whitelist"));
            score -= 0.4;
        }

        // Only apply stars filter if stars > 0 (i.e. data is available)
        if stars > 0 && stars < self.config.min_stars {
            reasons.push(format!(
                "stars {} below minimum {}",
                stars, self.config.min_stars
            ));
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

        if added_lines > 0 && added_lines < self.config.min_added_lines {
            reasons.push(format!(
                "added lines {} below minimum {}",
                added_lines, self.config.min_added_lines
            ));
            score -= 0.2;
        }

        if added_lines > self.config.max_added_lines {
            reasons.push(format!(
                "added lines {} above maximum {}",
                added_lines, self.config.max_added_lines
            ));
            score -= 0.2;
        }

        if !changed_files.is_empty() && Self::is_docs_only_change(changed_files) {
            reasons.push("all changed files are documentation/config only".to_string());
            score -= 0.3;
        }

        // Reject PRs with empty or very short descriptions
        let description_len = title.trim().len() + body.trim().len();
        if description_len < self.config.min_description_length {
            reasons.push(format!(
                "PR description too short ({description_len} chars, minimum {})",
                self.config.min_description_length
            ));
            score -= 0.4;
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

    fn is_docs_only_change(files: &[String]) -> bool {
        let doc_extensions = [
            "md", "txt", "yml", "yaml", "json", "toml", "ini", "cfg", "rst", "adoc", "csv", "svg",
            "png", "jpg", "jpeg", "gif", "ico",
        ];
        let doc_names = [
            "readme",
            "changelog",
            "license",
            "licence",
            "contributing",
            "authors",
            "codeowners",
            "code_of_conduct",
            ".gitignore",
            ".editorconfig",
            ".prettierrc",
            ".eslintignore",
        ];

        files.iter().all(|f| {
            let lower = f.to_lowercase();
            let basename = lower.rsplit('/').next().unwrap_or(&lower);
            let ext = basename.rsplit('.').next().unwrap_or("");

            doc_extensions.contains(&ext) || doc_names.iter().any(|n| basename.starts_with(n))
        })
    }
}
