//! JSON extraction utilities for parsing LLM responses.
//!
//! This module provides robust JSON extraction from LLM responses that may contain
//! markdown code blocks, explanatory text, or other mixed content. It implements
//! multiple extraction strategies to handle various response formats.
//!
//! # Extraction Strategies
//!
//! The extraction functions try the following strategies in order:
//! 1. Direct JSON (content starts with '{' or '[')
//! 2. JSON in markdown code blocks
//! 3. JSON in generic code blocks
//! 4. JSON object/array anywhere in content using bracket matching
//! 5. Regex-based extraction for complex/malformed cases
//!
//! # Example
//!
//! ```
//! use swe_forge::utils::json_extraction::extract_json_from_response;
//!
//! // Simple JSON object extraction
//! let response = "Here is the result: {\"name\": \"example\", \"value\": 42}";
//! let json = extract_json_from_response(response);
//! assert!(json.contains("example"));
//!
//! // JSON array extraction
//! let array_response = "[1, 2, 3]";
//! let array_json = extract_json_from_response(array_response);
//! assert_eq!(array_json, "[1, 2, 3]");
//! ```

use regex::Regex;
use thiserror::Error;

/// Error type for JSON extraction failures
#[derive(Debug, Clone, Error, PartialEq)]
pub enum JsonExtractionError {
    #[error("JSON appears truncated: {unclosed_braces} unclosed braces, {unclosed_brackets} unclosed brackets. Partial: {partial_preview}...")]
    Truncated {
        partial_preview: String,
        unclosed_braces: usize,
        unclosed_brackets: usize,
    },
    #[error("No JSON content found in response. Content starts with: '{content_preview}'")]
    NotFound { content_preview: String },
}

/// Result of JSON extraction attempt
#[derive(Debug, Clone, PartialEq)]
pub enum JsonExtractionResult {
    /// Successfully extracted valid JSON
    Success(String),
    /// JSON appears to be truncated (started but didn't complete)
    Truncated {
        partial_json: String,
        unclosed_braces: usize,
        unclosed_brackets: usize,
    },
    /// No JSON-like content found in response
    NotFound,
}

impl JsonExtractionResult {
    /// Returns true if JSON was successfully extracted
    pub fn is_success(&self) -> bool {
        matches!(self, JsonExtractionResult::Success(_))
    }

    /// Returns true if JSON appears to be truncated
    pub fn is_truncated(&self) -> bool {
        matches!(self, JsonExtractionResult::Truncated { .. })
    }

    /// Returns the extracted JSON string for the Success case
    pub fn json(&self) -> Option<&str> {
        match self {
            JsonExtractionResult::Success(json) => Some(json),
            _ => None,
        }
    }

    /// Converts the result to a Result with a descriptive error
    pub fn into_result(self) -> Result<String, JsonExtractionError> {
        match self {
            JsonExtractionResult::Success(json) => Ok(json),
            JsonExtractionResult::Truncated {
                partial_json,
                unclosed_braces,
                unclosed_brackets,
            } => {
                let preview_len = partial_json.len().min(100);
                let partial_preview = partial_json[..preview_len].to_string();
                Err(JsonExtractionError::Truncated {
                    partial_preview,
                    unclosed_braces,
                    unclosed_brackets,
                })
            }
            JsonExtractionResult::NotFound => Err(JsonExtractionError::NotFound {
                content_preview: String::new(),
            }),
        }
    }

    /// Converts the result to a Result with a descriptive error, including content preview for NotFound
    pub fn into_result_with_context(self, content: &str) -> Result<String, JsonExtractionError> {
        match self {
            JsonExtractionResult::Success(json) => Ok(json),
            JsonExtractionResult::Truncated {
                partial_json,
                unclosed_braces,
                unclosed_brackets,
            } => {
                let preview_len = partial_json.len().min(100);
                let partial_preview = partial_json[..preview_len].to_string();
                Err(JsonExtractionError::Truncated {
                    partial_preview,
                    unclosed_braces,
                    unclosed_brackets,
                })
            }
            JsonExtractionResult::NotFound => {
                let trimmed = content.trim();
                let preview_len = trimmed.len().min(50);
                let content_preview = trimmed[..preview_len].to_string();
                Err(JsonExtractionError::NotFound { content_preview })
            }
        }
    }
}

/// Analysis result for JSON structure
#[derive(Debug, Clone, PartialEq)]
pub struct JsonStructureAnalysis {
    /// Number of unclosed braces ('{' without matching '}')
    pub unclosed_braces: usize,
    /// Number of unclosed brackets ('[' without matching ']')
    pub unclosed_brackets: usize,
    /// Whether we ended inside a string literal
    pub in_string: bool,
    /// The position where JSON-like content starts (first '{' or '[')
    pub json_start: Option<usize>,
}

/// Analyzes JSON structure to determine if content is truncated
///
/// This function scans the content and tracks brace/bracket depth to detect
/// incomplete JSON structures.
///
/// # Arguments
///
/// * `s` - The string to analyze
///
/// # Returns
///
/// A `JsonStructureAnalysis` with details about unclosed braces/brackets
pub fn analyze_json_structure(s: &str) -> JsonStructureAnalysis {
    let mut brace_depth: isize = 0;
    let mut bracket_depth: isize = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let mut json_start: Option<usize> = None;

    for (i, c) in s.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => {
                escape_next = true;
            }
            '"' => {
                in_string = !in_string;
            }
            '{' if !in_string => {
                if json_start.is_none() {
                    json_start = Some(i);
                }
                brace_depth += 1;
            }
            '}' if !in_string => {
                brace_depth -= 1;
            }
            '[' if !in_string => {
                if json_start.is_none() {
                    json_start = Some(i);
                }
                bracket_depth += 1;
            }
            ']' if !in_string => {
                bracket_depth -= 1;
            }
            _ => {}
        }
    }

    JsonStructureAnalysis {
        unclosed_braces: brace_depth.max(0) as usize,
        unclosed_brackets: bracket_depth.max(0) as usize,
        in_string,
        json_start,
    }
}

/// Detects if content appears to contain truncated JSON
///
/// Returns Some((partial, unclosed_braces, unclosed_brackets)) if truncated JSON is detected,
/// None if JSON is complete or no JSON is found.
///
/// # Arguments
///
/// * `content` - The content to analyze
///
/// # Returns
///
/// Some tuple with (partial_json, unclosed_braces, unclosed_brackets) if truncated,
/// None otherwise
pub fn detect_truncated_json(content: &str) -> Option<(String, usize, usize)> {
    let trimmed = content.trim();
    let analysis = analyze_json_structure(trimmed);

    // If there's no JSON start, there's nothing to detect
    let json_start = analysis.json_start?;

    // If there are unclosed braces or brackets, JSON is truncated
    if analysis.unclosed_braces > 0 || analysis.unclosed_brackets > 0 || analysis.in_string {
        let partial_json = trimmed[json_start..].to_string();
        Some((
            partial_json,
            analysis.unclosed_braces,
            analysis.unclosed_brackets,
        ))
    } else {
        None
    }
}

/// Attempts to extract JSON from an LLM response with detailed result information.
///
/// This function provides detailed information about the extraction attempt,
/// distinguishing between successful extraction, truncated JSON, and no JSON found.
///
/// The extraction uses the following strategy order (optimized for reasoning models
/// like Kimi K2.5 that may include thinking content before JSON):
/// 1. Markdown code blocks (```json ... ```) - most reliable for structured output
/// 2. Generic code blocks (``` ... ```)
/// 3. Direct JSON if content starts with '{' or '['
/// 4. Find LAST valid JSON object in content (handles reasoning content before JSON)
/// 5. Find first JSON anywhere
/// 6. Regex-based extraction as fallback
///
/// # Arguments
///
/// * `content` - The raw LLM response content
///
/// # Returns
///
/// A `JsonExtractionResult` indicating success, truncation, or not found
pub fn try_extract_json_from_response(content: &str) -> JsonExtractionResult {
    let trimmed = content.trim();

    // Strategy 1: Try to find JSON block in markdown code fence FIRST
    // This is the most reliable for structured output from reasoning models
    if let Some(json) = extract_from_json_code_block(trimmed) {
        // Validate it parses as JSON
        if serde_json::from_str::<serde_json::Value>(&json).is_ok() {
            return JsonExtractionResult::Success(json);
        }
    }

    // Strategy 2: Try generic code block
    if let Some(json) = extract_from_generic_code_block(trimmed) {
        // Validate it parses as JSON
        if serde_json::from_str::<serde_json::Value>(&json).is_ok() {
            return JsonExtractionResult::Success(json);
        }
    }

    // Strategy 3a: If it already starts with '{', find the matching closing brace
    // and validate it parses as JSON
    if trimmed.starts_with('{') {
        if let Some(end) = find_matching_brace(trimmed) {
            let candidate = &trimmed[..=end];
            // Validate it parses as JSON
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                return JsonExtractionResult::Success(candidate.to_string());
            }
        }
        // Check if truncated
        let analysis = analyze_json_structure(trimmed);
        if analysis.unclosed_braces > 0 || analysis.unclosed_brackets > 0 || analysis.in_string {
            return JsonExtractionResult::Truncated {
                partial_json: trimmed.to_string(),
                unclosed_braces: analysis.unclosed_braces,
                unclosed_brackets: analysis.unclosed_brackets,
            };
        }
    }

    // Strategy 3b: If it starts with '[', find the matching closing bracket
    if trimmed.starts_with('[') {
        if let Some(end) = find_matching_bracket(trimmed) {
            let candidate = &trimmed[..=end];
            // Validate it parses as JSON
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                return JsonExtractionResult::Success(candidate.to_string());
            }
        }
        // Check if truncated
        let analysis = analyze_json_structure(trimmed);
        if analysis.unclosed_braces > 0 || analysis.unclosed_brackets > 0 || analysis.in_string {
            return JsonExtractionResult::Truncated {
                partial_json: trimmed.to_string(),
                unclosed_braces: analysis.unclosed_braces,
                unclosed_brackets: analysis.unclosed_brackets,
            };
        }
    }

    // Strategy 4: Try to find the LAST valid JSON object in content
    // This is important for reasoning models that output thinking content before JSON
    if let Some(json) = extract_last_valid_json_object(trimmed) {
        return JsonExtractionResult::Success(json);
    }

    // Strategy 5: Try to find JSON object anywhere using brace matching
    // (first occurrence)
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = find_matching_brace(&trimmed[start..]) {
            let candidate = &trimmed[start..=start + end];
            // Validate it parses as JSON
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                return JsonExtractionResult::Success(candidate.to_string());
            }
        }
        // Check if truncated from this position
        let analysis = analyze_json_structure(&trimmed[start..]);
        if analysis.unclosed_braces > 0 || analysis.unclosed_brackets > 0 || analysis.in_string {
            return JsonExtractionResult::Truncated {
                partial_json: trimmed[start..].to_string(),
                unclosed_braces: analysis.unclosed_braces,
                unclosed_brackets: analysis.unclosed_brackets,
            };
        }
    }

    // Strategy 5b: Try to find JSON array anywhere using bracket matching
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = find_matching_bracket(&trimmed[start..]) {
            let candidate = &trimmed[start..=start + end];
            // Validate it parses as JSON
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                return JsonExtractionResult::Success(candidate.to_string());
            }
        }
        // Check if truncated from this position
        let analysis = analyze_json_structure(&trimmed[start..]);
        if analysis.unclosed_braces > 0 || analysis.unclosed_brackets > 0 || analysis.in_string {
            return JsonExtractionResult::Truncated {
                partial_json: trimmed[start..].to_string(),
                unclosed_braces: analysis.unclosed_braces,
                unclosed_brackets: analysis.unclosed_brackets,
            };
        }
    }

    // Strategy 6: Try regex-based extraction as fallback
    if let Some(json) = extract_json_with_regex(trimmed) {
        return JsonExtractionResult::Success(json);
    }

    // Check for any JSON-like content that might be truncated
    if let Some((partial, unclosed_braces, unclosed_brackets)) = detect_truncated_json(trimmed) {
        return JsonExtractionResult::Truncated {
            partial_json: partial,
            unclosed_braces,
            unclosed_brackets,
        };
    }

    // No JSON found
    JsonExtractionResult::NotFound
}

/// Extracts JSON content from an LLM response that might be wrapped in markdown.
///
/// This is the main entry point for JSON extraction. It tries multiple strategies
/// to find valid JSON in the response, handling common LLM response patterns like
/// markdown code blocks and explanatory text.
///
/// # Arguments
///
/// * `content` - The raw LLM response content
///
/// # Returns
///
/// The extracted JSON string, or the trimmed original content if no JSON could be found.
///
/// # Note
///
/// For more detailed extraction results including truncation detection, use
/// `try_extract_json_from_response` instead.
pub fn extract_json_from_response(content: &str) -> String {
    let trimmed = content.trim();

    match try_extract_json_from_response(content) {
        JsonExtractionResult::Success(json) => json,
        JsonExtractionResult::Truncated { partial_json, .. } => partial_json,
        JsonExtractionResult::NotFound => trimmed.to_string(),
    }
}

/// Helper function to find the matching closing brace for a JSON object.
///
/// This function properly handles:
/// - Nested braces
/// - String literals (including escaped quotes)
/// - Escape sequences within strings
///
/// # Arguments
///
/// * `s` - A string starting with '{'
///
/// # Returns
///
/// The index of the matching closing '}', or None if not found.
pub fn find_matching_brace(s: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, c) in s.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => {
                escape_next = true;
            }
            '"' => {
                in_string = !in_string;
            }
            '{' if !in_string => {
                depth += 1;
            }
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }

    None
}

/// Helper function to find the matching closing bracket for a JSON array.
///
/// This function properly handles:
/// - Nested brackets and braces
/// - String literals (including escaped quotes)
/// - Escape sequences within strings
///
/// # Arguments
///
/// * `s` - A string starting with '['
///
/// # Returns
///
/// The index of the matching closing ']', or None if not found.
pub fn find_matching_bracket(s: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, c) in s.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match c {
            '\\' if in_string => {
                escape_next = true;
            }
            '"' => {
                in_string = !in_string;
            }
            '[' if !in_string => {
                depth += 1;
            }
            ']' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }

    None
}

/// Extract JSON from a ```json ... ``` code block.
///
/// # Arguments
///
/// * `content` - The content to search for a JSON code block
///
/// # Returns
///
/// The extracted JSON string if found, or None.
pub fn extract_from_json_code_block(content: &str) -> Option<String> {
    let re = Regex::new(r"```json\s*\n?([\s\S]*?)\n?```").ok()?;
    if let Some(caps) = re.captures(content) {
        let json_content = caps.get(1)?.as_str().trim();
        if json_content.starts_with('{') {
            if let Some(end) = find_matching_brace(json_content) {
                return Some(json_content[..=end].to_string());
            }
            return Some(json_content.to_string());
        }
    }
    None
}

/// Extract JSON from a generic ``` ... ``` code block.
///
/// # Arguments
///
/// * `content` - The content to search for a code block
///
/// # Returns
///
/// The extracted JSON string if found, or None.
pub fn extract_from_generic_code_block(content: &str) -> Option<String> {
    let re = Regex::new(r"```(?:\w+)?\s*\n?([\s\S]*?)\n?```").ok()?;
    if let Some(caps) = re.captures(content) {
        let block_content = caps.get(1)?.as_str().trim();
        if let Some(start) = block_content.find('{') {
            if let Some(end) = find_matching_brace(&block_content[start..]) {
                return Some(block_content[start..=start + end].to_string());
            }
        }
    }
    None
}

/// Extract the LARGEST valid JSON object from content, preferring later occurrences.
///
/// This is important for reasoning models (like Kimi K2.5) that output
/// thinking/reasoning content before the actual JSON response. The reasoning
/// content may contain JSON-like structures (e.g., examples, partial thoughts),
/// but the actual response JSON is typically:
/// 1. The largest JSON object (contains nested structures)
/// 2. Located towards the end of the response
///
/// # Arguments
///
/// * `content` - The content to search for JSON
///
/// # Returns
///
/// The extracted JSON string if found and valid, or None.
pub fn extract_last_valid_json_object(content: &str) -> Option<String> {
    // Find all positions of '{' in the content
    let brace_positions: Vec<usize> = content
        .char_indices()
        .filter_map(|(i, c)| if c == '{' { Some(i) } else { None })
        .collect();

    // Find all valid JSON objects and their sizes
    let mut valid_jsons: Vec<(usize, String)> = Vec::new();

    for &start in &brace_positions {
        let substr = &content[start..];
        if let Some(end) = find_matching_brace(substr) {
            let candidate = &substr[..=end];
            // Validate it parses as JSON
            if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                valid_jsons.push((start, candidate.to_string()));
            }
        }
    }

    if valid_jsons.is_empty() {
        return None;
    }

    // Prefer the largest JSON object (by character count)
    // If multiple have the same size, prefer the one that appears later
    valid_jsons
        .into_iter()
        .max_by(|(pos_a, json_a), (pos_b, json_b)| {
            // Primary sort: by size (larger is better)
            // Secondary sort: by position (later is better for same size)
            match json_a.len().cmp(&json_b.len()) {
                std::cmp::Ordering::Equal => pos_a.cmp(pos_b),
                other => other,
            }
        })
        .map(|(_, json)| json)
}

/// Extract JSON using regex as a fallback for complex cases.
///
/// This function handles malformed responses where JSON might be mixed with
/// other content in non-standard ways.
///
/// # Arguments
///
/// * `content` - The content to search for JSON
///
/// # Returns
///
/// The extracted JSON string if found and valid, or None.
pub fn extract_json_with_regex(content: &str) -> Option<String> {
    // First, try to find content between the first { and matching }
    let first_brace = content.find('{')?;
    let substr = &content[first_brace..];

    if let Some(end) = find_matching_brace(substr) {
        let candidate = &substr[..=end];
        // Validate it parses as JSON
        if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
            return Some(candidate.to_string());
        }
    }

    // Fallback: try from last { to last }
    let last_start = content.rfind('{')?;
    let last_end = content.rfind('}')?;
    if last_end > last_start {
        let candidate = &content[last_start..=last_end];
        if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
            return Some(candidate.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_direct_json() {
        let input = r#"{"key": "value"}"#;
        let result = extract_json_from_response(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_json_code_block() {
        let input = r#"Here is the response:
```json
{"key": "value"}
```
Hope this helps!"#;
        let result = extract_json_from_response(input);
        assert_eq!(result, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_generic_code_block() {
        let input = r#"Response:
```
{"key": "value"}
```"#;
        let result = extract_json_from_response(input);
        assert_eq!(result, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_json_with_text() {
        let input =
            r#"Sure, here's the JSON you requested: {"name": "test", "count": 5} - that's it!"#;
        let result = extract_json_from_response(input);
        assert_eq!(result, r#"{"name": "test", "count": 5}"#);
    }

    #[test]
    fn test_nested_json() {
        let input = r#"{"outer": {"inner": "value"}, "list": [1, 2, 3]}"#;
        let result = extract_json_from_response(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_json_with_escaped_quotes() {
        let input = r#"{"message": "He said \"hello\""}"#;
        let result = extract_json_from_response(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_find_matching_brace_simple() {
        let input = "{}";
        assert_eq!(find_matching_brace(input), Some(1));
    }

    #[test]
    fn test_find_matching_brace_nested() {
        let input = r#"{"a": {"b": "c"}}"#;
        assert_eq!(find_matching_brace(input), Some(16));
    }

    #[test]
    fn test_find_matching_brace_with_strings() {
        let input = r#"{"braces": "{ not a brace }"}"#;
        assert_eq!(find_matching_brace(input), Some(28));
    }

    #[test]
    fn test_json_array_direct() {
        let input = r#"[1, 2, 3]"#;
        let result = extract_json_from_response(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_json_array_objects() {
        let input = r#"[{"key": "value1"}, {"key": "value2"}]"#;
        let result = extract_json_from_response(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_json_array_with_text() {
        let input = r#"Here is the array: [1, 2, 3] - that's it!"#;
        let result = extract_json_from_response(input);
        assert_eq!(result, "[1, 2, 3]");
    }

    #[test]
    fn test_find_matching_bracket_simple() {
        let input = "[]";
        assert_eq!(find_matching_bracket(input), Some(1));
    }

    #[test]
    fn test_find_matching_bracket_nested() {
        let input = r#"[[1, 2], [3, 4]]"#;
        assert_eq!(find_matching_bracket(input), Some(15));
    }

    #[test]
    fn test_find_matching_bracket_with_objects() {
        let input = r#"[{"a": 1}, {"b": 2}]"#;
        assert_eq!(find_matching_bracket(input), Some(19));
    }

    // Tests for JsonExtractionResult helper methods
    #[test]
    fn test_json_extraction_result_is_success() {
        let success = JsonExtractionResult::Success("{}".to_string());
        assert!(success.is_success());
        assert!(!success.is_truncated());

        let truncated = JsonExtractionResult::Truncated {
            partial_json: "{".to_string(),
            unclosed_braces: 1,
            unclosed_brackets: 0,
        };
        assert!(!truncated.is_success());
        assert!(truncated.is_truncated());

        let not_found = JsonExtractionResult::NotFound;
        assert!(!not_found.is_success());
        assert!(!not_found.is_truncated());
    }

    #[test]
    fn test_json_extraction_result_json() {
        let success = JsonExtractionResult::Success(r#"{"key": "value"}"#.to_string());
        assert_eq!(success.json(), Some(r#"{"key": "value"}"#));

        let truncated = JsonExtractionResult::Truncated {
            partial_json: "{".to_string(),
            unclosed_braces: 1,
            unclosed_brackets: 0,
        };
        assert_eq!(truncated.json(), None);

        let not_found = JsonExtractionResult::NotFound;
        assert_eq!(not_found.json(), None);
    }

    #[test]
    fn test_json_extraction_result_into_result() {
        let success = JsonExtractionResult::Success(r#"{"key": "value"}"#.to_string());
        assert!(success.into_result().is_ok());

        let truncated = JsonExtractionResult::Truncated {
            partial_json: r#"{"key": "val"#.to_string(),
            unclosed_braces: 1,
            unclosed_brackets: 0,
        };
        let err = truncated.into_result().unwrap_err();
        assert!(matches!(err, JsonExtractionError::Truncated { .. }));

        let not_found = JsonExtractionResult::NotFound;
        let err = not_found.into_result().unwrap_err();
        assert!(matches!(err, JsonExtractionError::NotFound { .. }));
    }

    // Tests for truncated JSON detection
    #[test]
    fn test_truncated_json_simple_object() {
        // Missing closing brace
        let input = r#"{"key": "value""#;
        let result = try_extract_json_from_response(input);
        assert!(result.is_truncated());
        if let JsonExtractionResult::Truncated {
            unclosed_braces,
            unclosed_brackets,
            ..
        } = result
        {
            assert_eq!(unclosed_braces, 1);
            assert_eq!(unclosed_brackets, 0);
        }
    }

    #[test]
    fn test_truncated_json_nested() {
        // Nested incomplete JSON: {"outer": {"inner": "val
        let input = r#"{"outer": {"inner": "val"#;
        let result = try_extract_json_from_response(input);
        assert!(result.is_truncated());
        if let JsonExtractionResult::Truncated {
            unclosed_braces,
            unclosed_brackets,
            ..
        } = result
        {
            assert_eq!(unclosed_braces, 2);
            assert_eq!(unclosed_brackets, 0);
        }
    }

    #[test]
    fn test_truncated_json_array_with_objects() {
        // Array with truncated object: [{"a": 1}, {"b": 2
        let input = r#"[{"a": 1}, {"b": 2"#;
        let result = try_extract_json_from_response(input);
        assert!(result.is_truncated());
        if let JsonExtractionResult::Truncated {
            unclosed_braces,
            unclosed_brackets,
            ..
        } = result
        {
            assert_eq!(unclosed_braces, 1);
            assert_eq!(unclosed_brackets, 1);
        }
    }

    #[test]
    fn test_truncated_json_array_simple() {
        // Truncated array: [1, 2, 3
        let input = r#"[1, 2, 3"#;
        let result = try_extract_json_from_response(input);
        assert!(result.is_truncated());
        if let JsonExtractionResult::Truncated {
            unclosed_braces,
            unclosed_brackets,
            ..
        } = result
        {
            assert_eq!(unclosed_braces, 0);
            assert_eq!(unclosed_brackets, 1);
        }
    }

    #[test]
    fn test_truncated_json_with_text_prefix() {
        // JSON with text before it, but truncated
        let input = r#"Here is the response: {"name": "test"#;
        let result = try_extract_json_from_response(input);
        assert!(result.is_truncated());
        if let JsonExtractionResult::Truncated {
            partial_json,
            unclosed_braces,
            ..
        } = result
        {
            assert!(partial_json.starts_with('{'));
            assert_eq!(unclosed_braces, 1);
        }
    }

    #[test]
    fn test_truncated_json_in_markdown() {
        // JSON in markdown code block but truncated
        let input = r#"```json
{"key": "value
```"#;
        // The markdown extraction should fail because the JSON is incomplete
        let result = try_extract_json_from_response(input);
        // This will try to extract from markdown first, which returns the partial
        // Then it should detect truncation in raw content
        assert!(result.is_truncated() || result.is_success());
    }

    #[test]
    fn test_valid_json_still_works() {
        // Ensure valid JSON extraction still works
        let input = r#"{"key": "value", "nested": {"a": 1}}"#;
        let result = try_extract_json_from_response(input);
        assert!(result.is_success());
        assert_eq!(result.json(), Some(input));
    }

    #[test]
    fn test_valid_array_still_works() {
        let input = r#"[1, 2, 3, {"key": "value"}]"#;
        let result = try_extract_json_from_response(input);
        assert!(result.is_success());
        assert_eq!(result.json(), Some(input));
    }

    #[test]
    fn test_no_json_content() {
        let input = "This is just plain text with no JSON";
        let result = try_extract_json_from_response(input);
        assert!(matches!(result, JsonExtractionResult::NotFound));
    }

    #[test]
    fn test_empty_content() {
        let input = "";
        let result = try_extract_json_from_response(input);
        assert!(matches!(result, JsonExtractionResult::NotFound));

        let input_whitespace = "   \n\t  ";
        let result_whitespace = try_extract_json_from_response(input_whitespace);
        assert!(matches!(result_whitespace, JsonExtractionResult::NotFound));
    }

    #[test]
    fn test_analyze_json_structure() {
        // Complete JSON
        let complete = r#"{"key": "value"}"#;
        let analysis = analyze_json_structure(complete);
        assert_eq!(analysis.unclosed_braces, 0);
        assert_eq!(analysis.unclosed_brackets, 0);
        assert!(!analysis.in_string);
        assert_eq!(analysis.json_start, Some(0));

        // Truncated with unclosed brace
        let truncated = r#"{"key": "value""#;
        let analysis = analyze_json_structure(truncated);
        assert_eq!(analysis.unclosed_braces, 1);
        assert_eq!(analysis.unclosed_brackets, 0);
        assert!(!analysis.in_string);

        // Truncated mid-string
        let mid_string = r#"{"key": "val"#;
        let analysis = analyze_json_structure(mid_string);
        assert_eq!(analysis.unclosed_braces, 1);
        assert!(analysis.in_string);
    }

    #[test]
    fn test_detect_truncated_json() {
        // Truncated JSON
        let truncated = r#"{"key": "value""#;
        let result = detect_truncated_json(truncated);
        assert!(result.is_some());
        let (partial, unclosed_braces, unclosed_brackets) = result.unwrap();
        assert!(partial.starts_with('{'));
        assert_eq!(unclosed_braces, 1);
        assert_eq!(unclosed_brackets, 0);

        // Complete JSON returns None
        let complete = r#"{"key": "value"}"#;
        let result = detect_truncated_json(complete);
        assert!(result.is_none());

        // No JSON returns None
        let no_json = "plain text";
        let result = detect_truncated_json(no_json);
        assert!(result.is_none());
    }

    #[test]
    fn test_error_messages() {
        // Test error message formatting
        let truncated_err = JsonExtractionError::Truncated {
            partial_preview: r#"{"key": "val"#.to_string(),
            unclosed_braces: 1,
            unclosed_brackets: 0,
        };
        let err_msg = truncated_err.to_string();
        assert!(err_msg.contains("truncated"));
        assert!(err_msg.contains("1 unclosed braces"));
        assert!(err_msg.contains("0 unclosed brackets"));

        let not_found_err = JsonExtractionError::NotFound {
            content_preview: "Hello world".to_string(),
        };
        let err_msg = not_found_err.to_string();
        assert!(err_msg.contains("No JSON content found"));
        assert!(err_msg.contains("Hello world"));
    }

    #[test]
    fn test_backward_compatibility() {
        // Ensure extract_json_from_response still works for truncated input
        // It should return the partial JSON (backward compatible behavior)
        let truncated = r#"{"key": "value""#;
        let result = extract_json_from_response(truncated);
        assert!(result.starts_with('{'));

        // Complete JSON still extracts correctly
        let complete = r#"{"key": "value"}"#;
        let result = extract_json_from_response(complete);
        assert_eq!(result, complete);
    }

    #[test]
    fn test_into_result_with_context() {
        let not_found = JsonExtractionResult::NotFound;
        let content = "Some plain text content here";
        let err = not_found.into_result_with_context(content).unwrap_err();
        if let JsonExtractionError::NotFound { content_preview } = err {
            assert!(content_preview.contains("Some plain text"));
        } else {
            panic!("Expected NotFound error");
        }
    }

    #[test]
    fn test_deeply_nested_truncated() {
        // Deeply nested truncated JSON
        let input = r#"{"a": {"b": {"c": {"d": "value"#;
        let result = try_extract_json_from_response(input);
        assert!(result.is_truncated());
        if let JsonExtractionResult::Truncated {
            unclosed_braces, ..
        } = result
        {
            assert_eq!(unclosed_braces, 4);
        }
    }

    #[test]
    fn test_mixed_braces_and_brackets_truncated() {
        // Mix of unclosed braces and brackets
        let input = r#"{"items": [{"id": 1}, {"id": 2"#;
        let result = try_extract_json_from_response(input);
        assert!(result.is_truncated());
        if let JsonExtractionResult::Truncated {
            unclosed_braces,
            unclosed_brackets,
            ..
        } = result
        {
            assert_eq!(unclosed_braces, 2); // outer object + inner incomplete object
            assert_eq!(unclosed_brackets, 1); // unclosed array
        }
    }

    #[test]
    fn test_reasoning_model_output_with_thinking_before_json() {
        // Simulate reasoning model output where thinking content comes before JSON
        // This is common with Kimi K2.5 and similar models
        let input = r#"Let me think about this step by step.

First, I need to analyze the task requirements. The user wants me to generate a task with {difficulty: "hard"} parameters.

Now let me construct the proper response:

{"problem_statement": "Analyze the log file", "difficulty": {"level": "hard", "complexity_factors": ["multi-step", "domain-knowledge"]}, "tags": ["debugging", "logs"]}"#;

        let result = try_extract_json_from_response(input);
        assert!(result.is_success());

        // Should extract the last valid JSON object (the actual response)
        let json = result.json().unwrap();
        assert!(json.contains("problem_statement"));
        assert!(json.contains("Analyze the log file"));
    }

    #[test]
    fn test_reasoning_model_output_with_example_json_in_thinking() {
        // Reasoning content contains example JSON that should be skipped
        let input = r#"I'm thinking about what structure to use.

An example response might look like: {"example": "value"}

But the actual response should be more complete:

{"id": "task-001", "name": "Real Task", "data": {"key": "actual value"}}"#;

        let result = try_extract_json_from_response(input);
        assert!(result.is_success());

        // Should extract the last valid JSON object
        let json = result.json().unwrap();
        assert!(json.contains("task-001"));
        assert!(json.contains("Real Task"));
    }

    #[test]
    fn test_extract_last_valid_json_object() {
        // Multiple JSON objects of same size, should get the last one
        // Note: all objects have same length to test position-based tiebreaker
        let input = r#"{"a": 1} some text {"b": 2} more text {"c": 3}"#;

        let result = extract_last_valid_json_object(input);
        assert!(result.is_some());
        // Same size objects - should get the last one
        assert_eq!(result.unwrap(), r#"{"c": 3}"#);
    }

    #[test]
    fn test_extract_last_valid_json_object_prefers_larger() {
        // Larger JSON object should be preferred even if it appears earlier
        let input = r#"{"big": {"nested": "value", "count": 42}} text {"small": 1}"#;

        let result = extract_last_valid_json_object(input);
        assert!(result.is_some());
        // Should get the larger object
        let json = result.unwrap();
        assert!(json.contains("nested"));
        assert!(json.contains("count"));
    }

    #[test]
    fn test_extract_last_valid_json_object_with_invalid_in_between() {
        // First JSON is valid, middle is invalid, last is valid
        let input = r#"{"valid": 1} {invalid json {"final": "answer"}"#;

        let result = extract_last_valid_json_object(input);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), r#"{"final": "answer"}"#);
    }

    #[test]
    fn test_reasoning_model_with_markdown_json_block() {
        // Reasoning model wraps actual output in markdown code block
        let input = r#"Let me think about this...

After careful analysis, here's the response:

```json
{"problem_statement": "Debug the application", "status": "ready"}
```

Hope this helps!"#;

        let result = try_extract_json_from_response(input);
        assert!(result.is_success());

        // Should extract from markdown block
        let json = result.json().unwrap();
        assert!(json.contains("Debug the application"));
    }

    #[test]
    fn test_json_validation_prevents_incomplete_extraction() {
        // Content starts with { but first match isn't valid JSON
        let input = r#"{some invalid content} followed by {"valid": "json"}"#;

        let result = try_extract_json_from_response(input);
        assert!(result.is_success());

        // Should find the valid JSON object
        let json = result.json().unwrap();
        assert_eq!(json, r#"{"valid": "json"}"#);
    }
}
