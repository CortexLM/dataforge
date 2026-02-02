use std::collections::HashMap;

use crate::error::RegistryError;
use crate::registry::entry::{TaskRegistryEntry, TaskStatus};

/// Minimum number of testers required for review approval.
const MIN_TESTERS_FOR_REVIEW: u32 = 3;

/// Minimum success rate required for publish approval.
const MIN_SUCCESS_RATE_FOR_PUBLISH: f64 = 0.5;

/// Manages task lifecycle state transitions.
///
/// Enforces valid state transitions and requirements for moving
/// tasks between lifecycle stages.
pub struct LifecycleManager {
    valid_transitions: HashMap<TaskStatus, Vec<TaskStatus>>,
}

impl LifecycleManager {
    /// Create a new lifecycle manager with standard transition rules.
    ///
    /// Valid transitions:
    /// - Draft -> Review (for initial review)
    /// - Draft -> Deprecated (to abandon a draft)
    /// - Review -> Draft (to return for revisions)
    /// - Review -> Published (to approve and publish)
    /// - Review -> Deprecated (to reject)
    /// - Published -> Deprecated (to retire)
    pub fn new() -> Self {
        let mut valid_transitions = HashMap::new();

        valid_transitions.insert(
            TaskStatus::Draft,
            vec![TaskStatus::Review, TaskStatus::Deprecated],
        );

        valid_transitions.insert(
            TaskStatus::Review,
            vec![
                TaskStatus::Draft,
                TaskStatus::Published,
                TaskStatus::Deprecated,
            ],
        );

        valid_transitions.insert(TaskStatus::Published, vec![TaskStatus::Deprecated]);

        valid_transitions.insert(TaskStatus::Deprecated, vec![]);

        Self { valid_transitions }
    }

    /// Check if a transition between two statuses is allowed.
    pub fn can_transition(&self, from: TaskStatus, to: TaskStatus) -> bool {
        self.valid_transitions
            .get(&from)
            .map(|targets| targets.contains(&to))
            .unwrap_or(false)
    }

    /// Attempt to transition a task to a new status.
    ///
    /// # Arguments
    /// * `entry` - The task entry to transition
    /// * `new_status` - The target status
    /// * `reason` - Reason for the transition (for audit trail)
    /// * `reviewer` - Optional reviewer identifier
    ///
    /// # Errors
    /// Returns an error if:
    /// - The transition is not allowed
    /// - Requirements for the target status are not met
    pub fn transition(
        &self,
        entry: &mut TaskRegistryEntry,
        new_status: TaskStatus,
        reason: &str,
        reviewer: Option<&str>,
    ) -> Result<(), RegistryError> {
        let current_status = entry.status;

        if !self.can_transition(current_status, new_status) {
            return Err(RegistryError::InvalidTransition {
                from: format!("{:?}", current_status),
                to: format!("{:?}", new_status),
                reason: format!(
                    "Transition from {:?} to {:?} is not allowed. Reason: {}",
                    current_status, new_status, reason
                ),
            });
        }

        // Check requirements based on target status
        match new_status {
            TaskStatus::Review => {
                self.check_review_requirements(entry)?;
            }
            TaskStatus::Published => {
                self.check_publish_requirements(entry)?;
            }
            TaskStatus::Draft | TaskStatus::Deprecated => {
                // No special requirements for returning to draft or deprecating
            }
        }

        // Perform the transition
        entry.status = new_status;

        // Update the metadata timestamp
        entry.metadata.updated_at = chrono::Utc::now().to_rfc3339();

        // Log the transition details (could be extended to write to audit log)
        let _reviewer_info = reviewer.unwrap_or("system");
        let _transition_info = format!(
            "Task {} transitioned from {:?} to {:?} by {}: {}",
            entry.id, current_status, new_status, _reviewer_info, reason
        );

        Ok(())
    }

    /// Check that requirements for entering Review status are met.
    fn check_review_requirements(&self, entry: &TaskRegistryEntry) -> Result<(), RegistryError> {
        // Task must have human calibration data
        if !entry.calibration.human_tested {
            return Err(RegistryError::ReviewRequirementsNotMet(
                "Task must be human tested before review".to_string(),
            ));
        }

        // Must have minimum number of testers
        if entry.calibration.num_testers < MIN_TESTERS_FOR_REVIEW {
            return Err(RegistryError::ReviewRequirementsNotMet(format!(
                "Task must have at least {} testers (has {})",
                MIN_TESTERS_FOR_REVIEW, entry.calibration.num_testers
            )));
        }

        Ok(())
    }

    /// Check that requirements for entering Published status are met.
    fn check_publish_requirements(&self, entry: &TaskRegistryEntry) -> Result<(), RegistryError> {
        // Must meet review requirements first
        self.check_review_requirements(entry)?;

        // Must have acceptable success rate
        if entry.calibration.success_rate < MIN_SUCCESS_RATE_FOR_PUBLISH {
            return Err(RegistryError::PublishRequirementsNotMet(format!(
                "Task success rate ({:.1}%) must be at least {:.1}%",
                entry.calibration.success_rate * 100.0,
                MIN_SUCCESS_RATE_FOR_PUBLISH * 100.0
            )));
        }

        // Must have at least one base image defined
        if entry.compatibility.base_images.is_empty() {
            return Err(RegistryError::PublishRequirementsNotMet(
                "Task must have at least one base image defined".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::entry::{Calibration, TaskMetadata};

    fn create_test_metadata() -> TaskMetadata {
        TaskMetadata {
            difficulty: "medium".to_string(),
            difficulty_score: 0.5,
            category: "debugging".to_string(),
            subcategory: "runtime".to_string(),
            tags: vec!["rust".to_string()],
            author: "test".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    fn create_test_entry() -> TaskRegistryEntry {
        TaskRegistryEntry::new(
            "test-task".to_string(),
            "template-001".to_string(),
            42,
            create_test_metadata(),
        )
    }

    #[test]
    fn test_valid_transitions() {
        let manager = LifecycleManager::new();

        // Draft transitions
        assert!(manager.can_transition(TaskStatus::Draft, TaskStatus::Review));
        assert!(manager.can_transition(TaskStatus::Draft, TaskStatus::Deprecated));
        assert!(!manager.can_transition(TaskStatus::Draft, TaskStatus::Published));

        // Review transitions
        assert!(manager.can_transition(TaskStatus::Review, TaskStatus::Draft));
        assert!(manager.can_transition(TaskStatus::Review, TaskStatus::Published));
        assert!(manager.can_transition(TaskStatus::Review, TaskStatus::Deprecated));

        // Published transitions
        assert!(manager.can_transition(TaskStatus::Published, TaskStatus::Deprecated));
        assert!(!manager.can_transition(TaskStatus::Published, TaskStatus::Draft));
        assert!(!manager.can_transition(TaskStatus::Published, TaskStatus::Review));

        // Deprecated is terminal
        assert!(!manager.can_transition(TaskStatus::Deprecated, TaskStatus::Draft));
        assert!(!manager.can_transition(TaskStatus::Deprecated, TaskStatus::Review));
        assert!(!manager.can_transition(TaskStatus::Deprecated, TaskStatus::Published));
    }

    #[test]
    fn test_transition_without_requirements() {
        let manager = LifecycleManager::new();
        let mut entry = create_test_entry();

        // Should fail because no human testing
        let result = manager.transition(&mut entry, TaskStatus::Review, "test", None);
        assert!(result.is_err());

        // Deprecating should always work from draft
        let result = manager.transition(&mut entry, TaskStatus::Deprecated, "abandoned", None);
        assert!(result.is_ok());
        assert_eq!(entry.status, TaskStatus::Deprecated);
    }

    #[test]
    fn test_transition_to_review_with_requirements() {
        let manager = LifecycleManager::new();
        let mut entry = create_test_entry();

        // Set up calibration data
        entry.calibration = Calibration {
            human_tested: true,
            num_testers: 5,
            avg_time_seconds: 120.0,
            success_rate: 0.8,
            last_calibration: Some("2024-01-01T00:00:00Z".to_string()),
        };

        let result = manager.transition(&mut entry, TaskStatus::Review, "ready for review", None);
        assert!(result.is_ok());
        assert_eq!(entry.status, TaskStatus::Review);
    }

    #[test]
    fn test_transition_to_published_requires_base_images() {
        let manager = LifecycleManager::new();
        let mut entry = create_test_entry();
        entry.status = TaskStatus::Review;

        entry.calibration = Calibration {
            human_tested: true,
            num_testers: 5,
            avg_time_seconds: 120.0,
            success_rate: 0.8,
            last_calibration: Some("2024-01-01T00:00:00Z".to_string()),
        };

        // Should fail without base images
        let result = manager.transition(&mut entry, TaskStatus::Published, "publish", None);
        assert!(result.is_err());

        // Add base image and try again
        entry.compatibility.base_images = vec!["rust:latest".to_string()];
        let result = manager.transition(&mut entry, TaskStatus::Published, "publish", None);
        assert!(result.is_ok());
        assert_eq!(entry.status, TaskStatus::Published);
    }
}
