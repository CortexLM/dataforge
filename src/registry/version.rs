use serde::{Deserialize, Serialize};

/// Keywords in change descriptions that indicate major version changes.
const MAJOR_CHANGE_KEYWORDS: &[&str] = &[
    "breaking",
    "incompatible",
    "removed",
    "major",
    "redesign",
    "overhaul",
];

/// Keywords in change descriptions that indicate minor version changes.
const MINOR_CHANGE_KEYWORDS: &[&str] = &[
    "added",
    "new feature",
    "enhanced",
    "minor",
    "improved",
    "feature",
];

/// Type of version increment to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionIncrement {
    /// Incompatible API changes (X.0.0)
    Major,
    /// Backwards-compatible functionality additions (0.X.0)
    Minor,
    /// Backwards-compatible bug fixes (0.0.X)
    Patch,
}

/// Policy for determining version increments based on changes.
pub struct VersionPolicy;

impl VersionPolicy {
    /// Determine the appropriate version increment based on change descriptions.
    ///
    /// Analyzes change descriptions for keywords indicating the scope of changes:
    /// - Breaking/incompatible changes -> Major
    /// - New features/enhancements -> Minor
    /// - Bug fixes/patches -> Patch
    pub fn determine_increment(changes: &[String]) -> VersionIncrement {
        for change in changes {
            let change_lower = change.to_lowercase();

            // Check for major version indicators
            for keyword in MAJOR_CHANGE_KEYWORDS {
                if change_lower.contains(keyword) {
                    return VersionIncrement::Major;
                }
            }
        }

        for change in changes {
            let change_lower = change.to_lowercase();

            // Check for minor version indicators
            for keyword in MINOR_CHANGE_KEYWORDS {
                if change_lower.contains(keyword) {
                    return VersionIncrement::Minor;
                }
            }
        }

        // Default to patch for bug fixes and small changes
        VersionIncrement::Patch
    }

    /// Increment a semantic version string according to the specified increment type.
    ///
    /// # Arguments
    /// * `current` - Current version in "major.minor.patch" format
    /// * `increment` - Type of increment to apply
    ///
    /// # Returns
    /// The new version string. Invalid formats return "1.0.0".
    pub fn increment_version(current: &str, increment: VersionIncrement) -> String {
        let parts: Vec<&str> = current.split('.').collect();

        if parts.len() != 3 {
            return "1.0.0".to_string();
        }

        let major: u32 = parts[0].parse().unwrap_or(1);
        let minor: u32 = parts[1].parse().unwrap_or(0);
        let patch: u32 = parts[2].parse().unwrap_or(0);

        match increment {
            VersionIncrement::Major => format!("{}.0.0", major + 1),
            VersionIncrement::Minor => format!("{}.{}.0", major, minor + 1),
            VersionIncrement::Patch => format!("{}.{}.{}", major, minor, patch + 1),
        }
    }
}

/// A single version release in the dataset history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRelease {
    /// Version number for this release.
    pub version: String,
    /// Previous version (empty string for initial release).
    pub previous_version: String,
    /// ISO 8601 timestamp of the release.
    pub timestamp: String,
    /// List of changes included in this release.
    pub changes: Vec<String>,
    /// Human-readable release notes.
    pub release_notes: String,
    /// Number of tasks in this release.
    pub task_count: usize,
}

/// Manages dataset versioning and release history.
pub struct DatasetVersion {
    /// Current version of the dataset.
    pub current_version: String,
    /// History of all version releases.
    pub version_history: Vec<VersionRelease>,
}

impl DatasetVersion {
    /// Create a new dataset version tracker starting at version 1.0.0.
    pub fn new() -> Self {
        Self {
            current_version: "1.0.0".to_string(),
            version_history: Vec::new(),
        }
    }

    /// Create a new release with the given changes.
    ///
    /// Automatically determines the version increment based on the change descriptions
    /// and records the release in version history.
    ///
    /// # Arguments
    /// * `changes` - List of change descriptions
    /// * `release_notes` - Human-readable notes about this release
    ///
    /// # Returns
    /// The new version number as a string.
    pub fn create_release(&mut self, changes: Vec<String>, release_notes: String) -> String {
        let increment = VersionPolicy::determine_increment(&changes);
        let new_version = VersionPolicy::increment_version(&self.current_version, increment);

        let release = VersionRelease {
            version: new_version.clone(),
            previous_version: self.current_version.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            changes,
            release_notes,
            task_count: 0, // Should be set by caller
        };

        self.version_history.push(release);
        self.current_version = new_version.clone();

        new_version
    }
}

impl Default for DatasetVersion {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_increment_major() {
        let changes = vec!["breaking: removed old API".to_string()];
        assert_eq!(
            VersionPolicy::determine_increment(&changes),
            VersionIncrement::Major
        );

        let changes = vec!["incompatible change to format".to_string()];
        assert_eq!(
            VersionPolicy::determine_increment(&changes),
            VersionIncrement::Major
        );
    }

    #[test]
    fn test_version_increment_minor() {
        let changes = vec!["added new task category".to_string()];
        assert_eq!(
            VersionPolicy::determine_increment(&changes),
            VersionIncrement::Minor
        );

        let changes = vec!["new feature: auto-calibration".to_string()];
        assert_eq!(
            VersionPolicy::determine_increment(&changes),
            VersionIncrement::Minor
        );
    }

    #[test]
    fn test_version_increment_patch() {
        let changes = vec!["fixed typo in task description".to_string()];
        assert_eq!(
            VersionPolicy::determine_increment(&changes),
            VersionIncrement::Patch
        );

        let changes = vec!["corrected validation logic".to_string()];
        assert_eq!(
            VersionPolicy::determine_increment(&changes),
            VersionIncrement::Patch
        );
    }

    #[test]
    fn test_increment_version() {
        assert_eq!(
            VersionPolicy::increment_version("1.2.3", VersionIncrement::Major),
            "2.0.0"
        );
        assert_eq!(
            VersionPolicy::increment_version("1.2.3", VersionIncrement::Minor),
            "1.3.0"
        );
        assert_eq!(
            VersionPolicy::increment_version("1.2.3", VersionIncrement::Patch),
            "1.2.4"
        );
    }

    #[test]
    fn test_increment_version_invalid_format() {
        assert_eq!(
            VersionPolicy::increment_version("invalid", VersionIncrement::Patch),
            "1.0.0"
        );
        assert_eq!(
            VersionPolicy::increment_version("1.2", VersionIncrement::Patch),
            "1.0.0"
        );
    }

    #[test]
    fn test_dataset_version_new() {
        let version = DatasetVersion::new();
        assert_eq!(version.current_version, "1.0.0");
        assert!(version.version_history.is_empty());
    }

    #[test]
    fn test_create_release() {
        let mut version = DatasetVersion::new();

        let changes = vec!["added new debugging tasks".to_string()];
        let new_ver = version.create_release(changes, "First minor release".to_string());

        assert_eq!(new_ver, "1.1.0");
        assert_eq!(version.current_version, "1.1.0");
        assert_eq!(version.version_history.len(), 1);

        let release = &version.version_history[0];
        assert_eq!(release.version, "1.1.0");
        assert_eq!(release.previous_version, "1.0.0");
        assert_eq!(release.release_notes, "First minor release");
    }

    #[test]
    fn test_multiple_releases() {
        let mut version = DatasetVersion::new();

        // Minor release
        version.create_release(
            vec!["added new feature".to_string()],
            "Feature release".to_string(),
        );
        assert_eq!(version.current_version, "1.1.0");

        // Patch release
        version.create_release(vec!["fixed bug".to_string()], "Bugfix release".to_string());
        assert_eq!(version.current_version, "1.1.1");

        // Major release
        version.create_release(
            vec!["breaking: new format".to_string()],
            "Major release".to_string(),
        );
        assert_eq!(version.current_version, "2.0.0");

        assert_eq!(version.version_history.len(), 3);
    }
}
