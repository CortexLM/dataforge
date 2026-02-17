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
            min_description_length: 80,
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

        // Reject PRs that only modify test files
        if !changed_files.is_empty() && Self::is_test_only_change(changed_files) {
            reasons.push("all changed files are test files only".to_string());
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

        // Check for install infrastructure (dependency management files)
        if !changed_files.is_empty() && !Self::has_install_infrastructure(changed_files) {
            reasons.push("no dependency management files detected in changed files".to_string());
            score -= 0.15;
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

    /// Check if all changed files are test files only.
    ///
    /// PRs that only modify test files cannot produce valid `fail_to_pass` tests
    /// because the test IS the change.
    fn is_test_only_change(files: &[String]) -> bool {
        if files.is_empty() {
            return false;
        }
        files.iter().all(|f| Self::is_test_file(f))
    }

    /// Check if a file path looks like a test file.
    fn is_test_file(path: &str) -> bool {
        let lower = path.to_lowercase();
        let basename = lower.rsplit('/').next().unwrap_or(&lower);

        // Test file name patterns
        basename.starts_with("test_")
            || basename.starts_with("test.")
            || basename.ends_with("_test.py")
            || basename.ends_with("_test.go")
            || basename.ends_with("_test.rs")
            || basename.ends_with("_test.js")
            || basename.ends_with("_test.ts")
            || basename.ends_with(".test.js")
            || basename.ends_with(".test.ts")
            || basename.ends_with(".test.tsx")
            || basename.ends_with(".test.jsx")
            || basename.ends_with(".spec.js")
            || basename.ends_with(".spec.ts")
            || basename.ends_with(".spec.rs")
            || basename.ends_with("_spec.rb")
            || basename.ends_with("test.java")
            // Test directory patterns
            || lower.contains("/tests/")
            || lower.contains("/test/")
            || lower.contains("/__tests__/")
            || lower.contains("/spec/")
            // Test config files
            || basename == "conftest.py"
            || basename == "pytest.ini"
            || basename == "jest.config.js"
            || basename == "jest.config.ts"
    }

    /// Check if the changed files indicate presence of install/dependency
    /// management infrastructure in the project.
    ///
    /// Repos without standard dependency management are unlikely to produce
    /// tasks where the install step succeeds.
    fn has_install_infrastructure(files: &[String]) -> bool {
        let install_indicators = [
            "requirements.txt",
            "setup.py",
            "setup.cfg",
            "pyproject.toml",
            "package.json",
            "package-lock.json",
            "yarn.lock",
            "pnpm-lock.yaml",
            "go.mod",
            "go.sum",
            "cargo.toml",
            "cargo.lock",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "gemfile",
            "gemfile.lock",
            "makefile",
            "cmake",
            "cmakelists.txt",
        ];

        // Check if any changed file is an install indicator or if any changed
        // file's directory siblings might include them (heuristic: if the repo
        // has source files, it likely has build infrastructure).
        files.iter().any(|f| {
            let lower = f.to_lowercase();
            let basename = lower.rsplit('/').next().unwrap_or(&lower);
            install_indicators.contains(&basename)
        }) || files.iter().any(|f| {
            // If there are source code files, assume the repo has build infrastructure
            let lower = f.to_lowercase();
            lower.ends_with(".py")
                || lower.ends_with(".rs")
                || lower.ends_with(".go")
                || lower.ends_with(".java")
                || lower.ends_with(".js")
                || lower.ends_with(".ts")
                || lower.ends_with(".rb")
                || lower.ends_with(".kt")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_min_description_length() {
        let config = FilterConfig::default();
        assert_eq!(config.min_description_length, 80);
    }

    #[test]
    fn test_is_test_only_change() {
        assert!(SweepFilter::is_test_only_change(&[
            "tests/test_foo.py".to_string(),
            "tests/test_bar.py".to_string(),
        ]));

        assert!(!SweepFilter::is_test_only_change(&[
            "tests/test_foo.py".to_string(),
            "src/main.py".to_string(),
        ]));

        assert!(!SweepFilter::is_test_only_change(&[]));
    }

    #[test]
    fn test_is_test_file() {
        assert!(SweepFilter::is_test_file("tests/test_foo.py"));
        assert!(SweepFilter::is_test_file("src/components/Button.test.tsx"));
        assert!(SweepFilter::is_test_file("spec/models/user_spec.rb"));
        assert!(SweepFilter::is_test_file("__tests__/utils.test.js"));
        assert!(SweepFilter::is_test_file("conftest.py"));

        assert!(!SweepFilter::is_test_file("src/main.py"));
        assert!(!SweepFilter::is_test_file("lib/utils.rs"));
        assert!(!SweepFilter::is_test_file("README.md"));
    }

    #[test]
    fn test_has_install_infrastructure() {
        assert!(SweepFilter::has_install_infrastructure(&[
            "requirements.txt".to_string(),
            "src/main.py".to_string(),
        ]));

        assert!(SweepFilter::has_install_infrastructure(&[
            "src/main.py".to_string(),
        ]));

        assert!(SweepFilter::has_install_infrastructure(&[
            "package.json".to_string(),
        ]));

        assert!(!SweepFilter::has_install_infrastructure(&[
            "README.md".to_string(),
            "docs/guide.md".to_string(),
        ]));
    }

    #[test]
    fn test_reject_test_only_change() {
        let filter = SweepFilter::with_defaults();
        let result = filter.keep_candidate(
            "python",
            100,
            2,
            50,
            &[
                "tests/test_foo.py".to_string(),
                "tests/test_bar.py".to_string(),
            ],
            "Update test suite",
            "This PR updates the test suite with better coverage for the parser module and adds new edge case tests.",
        );
        assert!(!result.accepted);
        assert!(result.reasons.iter().any(|r| r.contains("test files only")));
    }

    #[test]
    fn test_accept_mixed_change() {
        let filter = SweepFilter::with_defaults();
        let result = filter.keep_candidate(
            "python",
            100,
            3,
            50,
            &[
                "src/parser.py".to_string(),
                "tests/test_parser.py".to_string(),
            ],
            "Fix parser bug",
            "This PR fixes a critical bug in the parser module where nested expressions were not handled correctly.",
        );
        assert!(result.accepted);
    }

    #[test]
    fn test_description_length_filter() {
        let filter = SweepFilter::with_defaults();
        let result = filter.keep_candidate(
            "python",
            100,
            2,
            50,
            &["src/main.py".to_string()],
            "Fix bug",
            "Short desc",
        );
        assert!(!result.accepted);
        assert!(result.reasons.iter().any(|r| r.contains("too short")));
    }

    #[test]
    fn test_no_install_infrastructure_penalized() {
        let filter = SweepFilter::with_defaults();
        let result = filter.keep_candidate(
            "python",
            100,
            2,
            50,
            &["README.md".to_string(), "docs/guide.md".to_string()],
            "Update documentation",
            "This PR updates the project documentation with comprehensive guides for new contributors and updated API references.",
        );
        // Should be rejected for docs-only AND no install infrastructure
        assert!(!result.accepted);
    }
}
