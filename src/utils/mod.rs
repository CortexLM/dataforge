//! Shared utility functions for dataforge.
//!
//! This module provides common utilities used across multiple modules,
//! including JSON extraction from LLM responses.

pub mod json_extraction;

pub use json_extraction::{
    analyze_json_structure, detect_truncated_json, extract_from_generic_code_block,
    extract_from_json_code_block, extract_json_from_response, extract_json_with_regex,
    find_matching_brace, find_matching_bracket, try_extract_json_from_response,
    JsonExtractionError, JsonExtractionResult, JsonStructureAnalysis,
};
