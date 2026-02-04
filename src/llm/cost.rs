//! Cost tracking system for LLM usage.
//!
//! This module provides comprehensive cost tracking for LLM API usage,
//! including daily and monthly budgets, per-model cost tracking, and
//! usage history.

use chrono::{DateTime, Datelike, Utc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

/// Cents per dollar for internal calculations.
/// Using cents avoids floating-point precision issues.
const CENTS_PER_DOLLAR: f64 = 100.0;

/// Cost tracker for monitoring LLM API usage and budgets.
///
/// All costs are tracked internally in cents (hundredths of a dollar) to avoid
/// floating-point precision issues. Public methods accept and return dollar amounts.
pub struct CostTracker {
    /// Daily budget in cents.
    daily_budget_cents: u64,
    /// Monthly budget in cents.
    monthly_budget_cents: u64,
    /// Amount spent today in cents (atomic for thread-safe updates).
    spent_today_cents: AtomicU64,
    /// Amount spent this month in cents (atomic for thread-safe updates).
    spent_month_cents: AtomicU64,
    /// Per-model cost tracking in cents.
    cost_by_model: RwLock<HashMap<String, u64>>,
    /// Complete usage history.
    usage_history: RwLock<Vec<UsageRecord>>,
    /// Current tracking day (for daily reset detection).
    tracking_day: RwLock<u32>,
    /// Current tracking month (for monthly reset detection).
    tracking_month: RwLock<u32>,
}

/// A single usage record for tracking LLM API calls.
#[derive(Debug, Clone)]
pub struct UsageRecord {
    /// Timestamp of the API call.
    pub timestamp: DateTime<Utc>,
    /// Model identifier used for the call.
    pub model: String,
    /// Number of input tokens consumed.
    pub input_tokens: u32,
    /// Number of output tokens generated.
    pub output_tokens: u32,
    /// Cost in cents for this call.
    pub cost_cents: u64,
    /// Optional task identifier for tracking.
    pub task_id: Option<String>,
}

/// Summary report of cost tracking data.
#[derive(Debug, Clone)]
pub struct CostReport {
    /// Total spent today in dollars.
    pub daily_spent: f64,
    /// Remaining daily budget in dollars.
    pub daily_remaining: f64,
    /// Total spent this month in dollars.
    pub monthly_spent: f64,
    /// Remaining monthly budget in dollars.
    pub monthly_remaining: f64,
    /// Cost breakdown by model in dollars.
    pub by_model: HashMap<String, f64>,
}

impl CostTracker {
    /// Create a new cost tracker with specified budgets.
    ///
    /// # Arguments
    ///
    /// * `daily_budget` - Daily budget in dollars
    /// * `monthly_budget` - Monthly budget in dollars
    ///
    /// # Example
    ///
    /// ```
    /// use dataforge::llm::cost::CostTracker;
    ///
    /// // Create tracker with $10/day and $100/month budget
    /// let tracker = CostTracker::new(10.0, 100.0);
    /// ```
    pub fn new(daily_budget: f64, monthly_budget: f64) -> Self {
        let now = Utc::now();
        Self {
            daily_budget_cents: dollars_to_cents(daily_budget),
            monthly_budget_cents: dollars_to_cents(monthly_budget),
            spent_today_cents: AtomicU64::new(0),
            spent_month_cents: AtomicU64::new(0),
            cost_by_model: RwLock::new(HashMap::new()),
            usage_history: RwLock::new(Vec::new()),
            tracking_day: RwLock::new(now.ordinal()),
            tracking_month: RwLock::new(now.month()),
        }
    }

    /// Record usage from an LLM API call.
    ///
    /// # Arguments
    ///
    /// * `model` - Model identifier
    /// * `input_tokens` - Number of input tokens
    /// * `output_tokens` - Number of output tokens
    /// * `cost_per_1m_input` - Cost per 1 million input tokens in dollars
    /// * `cost_per_1m_output` - Cost per 1 million output tokens in dollars
    ///
    /// # Example
    ///
    /// ```
    /// use dataforge::llm::cost::CostTracker;
    ///
    /// let tracker = CostTracker::new(10.0, 100.0);
    /// // Record usage: 1000 input tokens, 500 output tokens
    /// // at $3.00/1M input and $15.00/1M output
    /// tracker.record_usage("gpt-4", 1000, 500, 3.0, 15.0);
    /// ```
    pub fn record_usage(
        &self,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cost_per_1m_input: f64,
        cost_per_1m_output: f64,
    ) {
        self.maybe_reset_counters();

        // Calculate cost in cents
        // Formula: (tokens / 1_000_000) * cost_per_1m * 100 (to convert to cents)
        let input_cost_cents =
            ((input_tokens as f64 / 1_000_000.0) * cost_per_1m_input * CENTS_PER_DOLLAR) as u64;
        let output_cost_cents =
            ((output_tokens as f64 / 1_000_000.0) * cost_per_1m_output * CENTS_PER_DOLLAR) as u64;
        let total_cost_cents = input_cost_cents + output_cost_cents;

        // Update atomic counters
        self.spent_today_cents
            .fetch_add(total_cost_cents, Ordering::SeqCst);
        self.spent_month_cents
            .fetch_add(total_cost_cents, Ordering::SeqCst);

        // Update per-model tracking
        {
            let mut cost_by_model = self
                .cost_by_model
                .write()
                .expect("cost_by_model lock poisoned");
            *cost_by_model.entry(model.to_string()).or_insert(0) += total_cost_cents;
        }

        // Record in usage history
        let record = UsageRecord {
            timestamp: Utc::now(),
            model: model.to_string(),
            input_tokens,
            output_tokens,
            cost_cents: total_cost_cents,
            task_id: None,
        };

        {
            let mut history = self
                .usage_history
                .write()
                .expect("usage_history lock poisoned");
            history.push(record);
        }

        tracing::debug!(
            model = model,
            input_tokens = input_tokens,
            output_tokens = output_tokens,
            cost_cents = total_cost_cents,
            "Recorded LLM usage"
        );
    }

    /// Record usage with an associated task ID.
    ///
    /// # Arguments
    ///
    /// * `model` - Model identifier
    /// * `input_tokens` - Number of input tokens
    /// * `output_tokens` - Number of output tokens
    /// * `cost_per_1m_input` - Cost per 1 million input tokens in dollars
    /// * `cost_per_1m_output` - Cost per 1 million output tokens in dollars
    /// * `task_id` - Task identifier for tracking
    pub fn record_usage_with_task(
        &self,
        model: &str,
        input_tokens: u32,
        output_tokens: u32,
        cost_per_1m_input: f64,
        cost_per_1m_output: f64,
        task_id: &str,
    ) {
        self.maybe_reset_counters();

        let input_cost_cents =
            ((input_tokens as f64 / 1_000_000.0) * cost_per_1m_input * CENTS_PER_DOLLAR) as u64;
        let output_cost_cents =
            ((output_tokens as f64 / 1_000_000.0) * cost_per_1m_output * CENTS_PER_DOLLAR) as u64;
        let total_cost_cents = input_cost_cents + output_cost_cents;

        self.spent_today_cents
            .fetch_add(total_cost_cents, Ordering::SeqCst);
        self.spent_month_cents
            .fetch_add(total_cost_cents, Ordering::SeqCst);

        {
            let mut cost_by_model = self
                .cost_by_model
                .write()
                .expect("cost_by_model lock poisoned");
            *cost_by_model.entry(model.to_string()).or_insert(0) += total_cost_cents;
        }

        let record = UsageRecord {
            timestamp: Utc::now(),
            model: model.to_string(),
            input_tokens,
            output_tokens,
            cost_cents: total_cost_cents,
            task_id: Some(task_id.to_string()),
        };

        {
            let mut history = self
                .usage_history
                .write()
                .expect("usage_history lock poisoned");
            history.push(record);
        }

        tracing::debug!(
            model = model,
            input_tokens = input_tokens,
            output_tokens = output_tokens,
            cost_cents = total_cost_cents,
            task_id = task_id,
            "Recorded LLM usage with task"
        );
    }

    /// Check if current spending is over either daily or monthly budget.
    pub fn is_over_budget(&self) -> bool {
        self.maybe_reset_counters();
        let daily = self.spent_today_cents.load(Ordering::SeqCst);
        let monthly = self.spent_month_cents.load(Ordering::SeqCst);
        daily >= self.daily_budget_cents || monthly >= self.monthly_budget_cents
    }

    /// Check if daily budget is exceeded.
    pub fn is_over_daily_budget(&self) -> bool {
        self.maybe_reset_counters();
        self.spent_today_cents.load(Ordering::SeqCst) >= self.daily_budget_cents
    }

    /// Check if monthly budget is exceeded.
    pub fn is_over_monthly_budget(&self) -> bool {
        self.maybe_reset_counters();
        self.spent_month_cents.load(Ordering::SeqCst) >= self.monthly_budget_cents
    }

    /// Get amount spent today in dollars.
    pub fn daily_spent(&self) -> f64 {
        self.maybe_reset_counters();
        cents_to_dollars(self.spent_today_cents.load(Ordering::SeqCst))
    }

    /// Get amount spent this month in dollars.
    pub fn monthly_spent(&self) -> f64 {
        self.maybe_reset_counters();
        cents_to_dollars(self.spent_month_cents.load(Ordering::SeqCst))
    }

    /// Get remaining daily budget in dollars.
    pub fn daily_remaining(&self) -> f64 {
        self.maybe_reset_counters();
        let spent = self.spent_today_cents.load(Ordering::SeqCst);
        if spent >= self.daily_budget_cents {
            0.0
        } else {
            cents_to_dollars(self.daily_budget_cents - spent)
        }
    }

    /// Get remaining monthly budget in dollars.
    pub fn monthly_remaining(&self) -> f64 {
        self.maybe_reset_counters();
        let spent = self.spent_month_cents.load(Ordering::SeqCst);
        if spent >= self.monthly_budget_cents {
            0.0
        } else {
            cents_to_dollars(self.monthly_budget_cents - spent)
        }
    }

    /// Get a comprehensive cost report.
    pub fn get_cost_report(&self) -> CostReport {
        self.maybe_reset_counters();

        let daily_spent_cents = self.spent_today_cents.load(Ordering::SeqCst);
        let monthly_spent_cents = self.spent_month_cents.load(Ordering::SeqCst);

        let by_model = {
            let cost_by_model = self
                .cost_by_model
                .read()
                .expect("cost_by_model lock poisoned");
            cost_by_model
                .iter()
                .map(|(model, &cents)| (model.clone(), cents_to_dollars(cents)))
                .collect()
        };

        CostReport {
            daily_spent: cents_to_dollars(daily_spent_cents),
            daily_remaining: if daily_spent_cents >= self.daily_budget_cents {
                0.0
            } else {
                cents_to_dollars(self.daily_budget_cents - daily_spent_cents)
            },
            monthly_spent: cents_to_dollars(monthly_spent_cents),
            monthly_remaining: if monthly_spent_cents >= self.monthly_budget_cents {
                0.0
            } else {
                cents_to_dollars(self.monthly_budget_cents - monthly_spent_cents)
            },
            by_model,
        }
    }

    /// Get the complete usage history.
    pub fn usage_history(&self) -> Vec<UsageRecord> {
        let history = self
            .usage_history
            .read()
            .expect("usage_history lock poisoned");
        history.clone()
    }

    /// Get total number of usage records.
    pub fn usage_count(&self) -> usize {
        let history = self
            .usage_history
            .read()
            .expect("usage_history lock poisoned");
        history.len()
    }

    /// Get total tokens consumed (input + output).
    pub fn total_tokens(&self) -> u64 {
        let history = self
            .usage_history
            .read()
            .expect("usage_history lock poisoned");
        history
            .iter()
            .map(|r| (r.input_tokens + r.output_tokens) as u64)
            .sum()
    }

    /// Get the configured daily budget in dollars.
    pub fn daily_budget(&self) -> f64 {
        cents_to_dollars(self.daily_budget_cents)
    }

    /// Get the configured monthly budget in dollars.
    pub fn monthly_budget(&self) -> f64 {
        cents_to_dollars(self.monthly_budget_cents)
    }

    /// Check if day/month changed and reset counters accordingly.
    fn maybe_reset_counters(&self) {
        let now = Utc::now();
        let current_day = now.ordinal();
        let current_month = now.month();

        // Check and reset daily counter
        {
            let mut tracking_day = self
                .tracking_day
                .write()
                .expect("tracking_day lock poisoned");
            if *tracking_day != current_day {
                self.spent_today_cents.store(0, Ordering::SeqCst);
                *tracking_day = current_day;
                tracing::info!("Daily cost counter reset");
            }
        }

        // Check and reset monthly counter
        {
            let mut tracking_month = self
                .tracking_month
                .write()
                .expect("tracking_month lock poisoned");
            if *tracking_month != current_month {
                self.spent_month_cents.store(0, Ordering::SeqCst);
                {
                    let mut cost_by_model = self
                        .cost_by_model
                        .write()
                        .expect("cost_by_model lock poisoned");
                    cost_by_model.clear();
                }
                *tracking_month = current_month;
                tracing::info!("Monthly cost counter reset");
            }
        }
    }
}

/// Convert dollars to cents.
fn dollars_to_cents(dollars: f64) -> u64 {
    (dollars * CENTS_PER_DOLLAR).round() as u64
}

/// Convert cents to dollars.
fn cents_to_dollars(cents: u64) -> f64 {
    cents as f64 / CENTS_PER_DOLLAR
}

/// Calculate cost in cents for a given token usage.
pub fn calculate_cost_cents(
    input_tokens: u32,
    output_tokens: u32,
    cost_per_1m_input: f64,
    cost_per_1m_output: f64,
) -> u64 {
    let input_cost_cents =
        ((input_tokens as f64 / 1_000_000.0) * cost_per_1m_input * CENTS_PER_DOLLAR) as u64;
    let output_cost_cents =
        ((output_tokens as f64 / 1_000_000.0) * cost_per_1m_output * CENTS_PER_DOLLAR) as u64;
    input_cost_cents + output_cost_cents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_tracker_new() {
        let tracker = CostTracker::new(10.0, 100.0);
        assert_eq!(tracker.daily_budget(), 10.0);
        assert_eq!(tracker.monthly_budget(), 100.0);
        assert_eq!(tracker.daily_spent(), 0.0);
        assert_eq!(tracker.monthly_spent(), 0.0);
    }

    #[test]
    fn test_record_usage() {
        let tracker = CostTracker::new(10.0, 100.0);

        // Record usage: 1000 input tokens at $3/1M, 500 output tokens at $15/1M
        // Expected cost: (1000/1M * 3 * 100) + (500/1M * 15 * 100) cents
        // = 0.3 cents + 0.75 cents = 1.05 cents -> rounds to 1 cent
        // = 0.01 dollars (but due to integer truncation: 0 + 0 = 0)
        tracker.record_usage("gpt-4", 1000, 500, 3.0, 15.0);

        // For small token counts, the cost rounds down to 0 due to integer math
        // Use larger token count for meaningful test
        tracker.record_usage("gpt-4", 100_000, 50_000, 3.0, 15.0);

        // Now: (100000/1M * 3 * 100) + (50000/1M * 15 * 100) = 30 + 75 = 105 cents = $1.05
        assert!(tracker.daily_spent() >= 1.0);
        assert!(tracker.daily_spent() <= 1.10);
    }

    #[test]
    fn test_record_usage_large_tokens() {
        let tracker = CostTracker::new(100.0, 1000.0);

        // Record 1M input tokens at $3/1M, 1M output tokens at $15/1M
        // Expected: $3 + $15 = $18
        tracker.record_usage("gpt-4", 1_000_000, 1_000_000, 3.0, 15.0);

        assert!((tracker.daily_spent() - 18.0).abs() < 0.01);
    }

    #[test]
    fn test_is_over_budget() {
        let tracker = CostTracker::new(0.01, 1000.0);

        assert!(!tracker.is_over_budget());

        // Record enough usage to exceed daily budget (0.01 = 1 cent)
        tracker.record_usage("gpt-4", 1_000_000, 0, 1.0, 0.0);

        assert!(tracker.is_over_budget());
        assert!(tracker.is_over_daily_budget());
        assert!(!tracker.is_over_monthly_budget());
    }

    #[test]
    fn test_get_cost_report() {
        let tracker = CostTracker::new(10.0, 100.0);

        tracker.record_usage("gpt-4", 1_000_000, 500_000, 3.0, 15.0);
        tracker.record_usage("claude-3", 500_000, 250_000, 8.0, 24.0);

        let report = tracker.get_cost_report();

        assert!(report.daily_spent > 0.0);
        assert!(report.daily_remaining < 10.0);
        assert!(report.by_model.contains_key("gpt-4"));
        assert!(report.by_model.contains_key("claude-3"));
    }

    #[test]
    fn test_usage_history() {
        let tracker = CostTracker::new(10.0, 100.0);

        tracker.record_usage("model-a", 100, 50, 1.0, 2.0);
        tracker.record_usage("model-b", 200, 100, 1.0, 2.0);

        let history = tracker.usage_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].model, "model-a");
        assert_eq!(history[1].model, "model-b");
    }

    #[test]
    fn test_record_usage_with_task() {
        let tracker = CostTracker::new(10.0, 100.0);

        tracker.record_usage_with_task("gpt-4", 1000, 500, 3.0, 15.0, "task-123");

        let history = tracker.usage_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].task_id, Some("task-123".to_string()));
    }

    #[test]
    fn test_total_tokens() {
        let tracker = CostTracker::new(10.0, 100.0);

        tracker.record_usage("model-a", 100, 50, 1.0, 2.0);
        tracker.record_usage("model-b", 200, 100, 1.0, 2.0);

        assert_eq!(tracker.total_tokens(), 450);
    }

    #[test]
    fn test_calculate_cost_cents() {
        // 1M input at $3/1M = 300 cents
        // 500K output at $15/1M = 750 cents
        // Total = 1050 cents
        let cost = calculate_cost_cents(1_000_000, 500_000, 3.0, 15.0);
        assert_eq!(cost, 1050);
    }

    #[test]
    fn test_remaining_budget() {
        let tracker = CostTracker::new(10.0, 100.0);

        assert_eq!(tracker.daily_remaining(), 10.0);
        assert_eq!(tracker.monthly_remaining(), 100.0);

        // Spend $5
        tracker.record_usage("gpt-4", 1_000_000, 1_000_000, 2.5, 2.5);

        assert!((tracker.daily_remaining() - 5.0).abs() < 0.01);
        assert!((tracker.monthly_remaining() - 95.0).abs() < 0.01);
    }

    #[test]
    fn test_dollars_to_cents_conversion() {
        assert_eq!(dollars_to_cents(1.0), 100);
        assert_eq!(dollars_to_cents(10.5), 1050);
        assert_eq!(dollars_to_cents(0.01), 1);
        assert_eq!(dollars_to_cents(0.0), 0);
    }

    #[test]
    fn test_cents_to_dollars_conversion() {
        assert_eq!(cents_to_dollars(100), 1.0);
        assert_eq!(cents_to_dollars(1050), 10.5);
        assert_eq!(cents_to_dollars(1), 0.01);
        assert_eq!(cents_to_dollars(0), 0.0);
    }
}
