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
//! use dataforge::utils::json_extraction::extract_json_from_response;
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
pub fn extract_json_from_response(content: &str) -> String {
    let trimmed = content.trim();

    // Strategy 1a: If it already starts with '{', find the matching closing brace
    if trimmed.starts_with('{') {
        if let Some(end) = find_matching_brace(trimmed) {
            return trimmed[..=end].to_string();
        }
    }

    // Strategy 1b: If it starts with '[', find the matching closing bracket (for JSON arrays)
    if trimmed.starts_with('[') {
        if let Some(end) = find_matching_bracket(trimmed) {
            return trimmed[..=end].to_string();
        }
    }

    // Strategy 2: Try to find JSON block in markdown code fence
    if let Some(json) = extract_from_json_code_block(trimmed) {
        return json;
    }

    // Strategy 3: Try generic code block
    if let Some(json) = extract_from_generic_code_block(trimmed) {
        return json;
    }

    // Strategy 4a: Try to find JSON object anywhere using brace matching
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = find_matching_brace(&trimmed[start..]) {
            return trimmed[start..=start + end].to_string();
        }
    }

    // Strategy 4b: Try to find JSON array anywhere using bracket matching
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = find_matching_bracket(&trimmed[start..]) {
            return trimmed[start..=start + end].to_string();
        }
    }

    // Strategy 5: Try regex-based extraction
    if let Some(json) = extract_json_with_regex(trimmed) {
        return json;
    }

    // Fallback: return the trimmed content as-is
    trimmed.to_string()
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
}
