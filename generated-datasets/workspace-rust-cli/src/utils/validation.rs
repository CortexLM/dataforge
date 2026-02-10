use crate::processor::parser::ParsedContent;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Content is empty")]
    EmptyContent,
    #[error("Content exceeds maximum length of {max} characters (found: {found})")]
    ContentTooLong { max: usize, found: usize },
    #[error("Invalid character at position {position}: '{character}'")]
    InvalidCharacter { position: usize, character: char },
    #[error("Missing required section: {section}")]
    MissingSectionError { section: String },
    #[error("Validation failed: {details}")]
    GenericError { details: String },
}

pub type ValidationResult = Result<(), ValidationError>;

pub fn validate_content(parsed: &ParsedContent) -> ValidationResult {
    validate_not_empty(&parsed.raw)?;
    validate_length(&parsed.raw, 10_000_000)?;
    validate_characters(&parsed.raw)?;
    
    Ok(())
}

pub fn validate_not_empty(content: &str) -> ValidationResult {
    if content.trim().is_empty() {
        return Err(ValidationError::EmptyContent);
    }
    Ok(())
}

pub fn validate_length(content: &str, max_length: usize) -> ValidationResult {
    if content.len() > max_length {
        return Err(ValidationError::ContentTooLong {
            max: max_length,
            found: content.len(),
        });
    }
    Ok(())
}

pub fn validate_characters(content: &str) -> ValidationResult {
    for (pos, ch) in content.chars().enumerate() {
        if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' {
            return Err(ValidationError::InvalidCharacter {
                position: pos,
                character: ch,
            });
        }
    }
    Ok(())
}

pub fn validate_sections(parsed: &ParsedContent, required: &[&str]) -> ValidationResult {
    for required_section in required {
        let found = parsed
            .sections
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(required_section));
        
        if !found {
            return Err(ValidationError::MissingSectionError {
                section: required_section.to_string(),
            });
        }
    }
    Ok(())
}

pub fn validate_line_count(parsed: &ParsedContent, min: usize, max: usize) -> ValidationResult {
    let count = parsed.metadata.line_count;
    if count < min || count > max {
        return Err(ValidationError::GenericError {
            details: format!(
                "Line count {} is outside allowed range [{}, {}]",
                count, min, max
            ),
        });
    }
    Ok(())
}

pub struct ContentValidator {
    rules: Vec<Box<dyn ValidationRule>>,
}

pub trait ValidationRule: Send + Sync {
    fn name(&self) -> &str;
    fn validate(&self, content: &str) -> ValidationResult;
}

struct MaxLengthRule {
    max_length: usize,
}

struct NoControlCharsRule;

struct MinWordCountRule {
    min_words: usize,
}

impl ValidationRule for MaxLengthRule {
    fn name(&self) -> &str {
        "max_length"
    }
    
    fn validate(&self, content: &str) -> ValidationResult {
        validate_length(content, self.max_length)
    }
}

impl ValidationRule for NoControlCharsRule {
    fn name(&self) -> &str {
        "no_control_chars"
    }
    
    fn validate(&self, content: &str) -> ValidationResult {
        validate_characters(content)
    }
}

impl ValidationRule for MinWordCountRule {
    fn name(&self) -> &str {
        "min_word_count"
    }
    
    fn validate(&self, content: &str) -> ValidationResult {
        let word_count = content.split_whitespace().count();
        if word_count < self.min_words {
            return Err(ValidationError::GenericError {
                details: format!(
                    "Word count {} is below minimum {}",
                    word_count, self.min_words
                ),
            });
        }
        Ok(())
    }
}

impl ContentValidator {
    pub fn new() -> Self {
        ContentValidator { rules: Vec::new() }
    }
    
    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.rules.push(Box::new(MaxLengthRule { max_length }));
        self
    }
    
    pub fn with_no_control_chars(mut self) -> Self {
        self.rules.push(Box::new(NoControlCharsRule));
        self
    }
    
    pub fn with_min_words(mut self, min_words: usize) -> Self {
        self.rules.push(Box::new(MinWordCountRule { min_words }));
        self
    }
    
    pub fn validate(&self, content: &str) -> ValidationResult {
        for rule in &self.rules {
            rule.validate(content)?;
        }
        Ok(())
    }
    
    pub fn validate_all(&self, content: &str) -> Vec<(String, ValidationResult)> {
        self.rules
            .iter()
            .map(|rule| (rule.name().to_string(), rule.validate(content)))
            .collect()
    }
}

impl Default for ContentValidator {
    fn default() -> Self {
        Self::new()
    }
}

pub fn calculate_content_score(content: &str) -> u32 {
    let base_score = 100u32;
    let length_penalty = (content.len() / 1000) as u32;
    let whitespace_ratio = content.chars().filter(|c| c.is_whitespace()).count() as u32
        * 100
        / content.len().max(1) as u32;
    
    base_score - length_penalty - whitespace_ratio / 10
}

pub fn sanitize_content(content: &str) -> String {
    content
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

pub fn normalize_line_endings(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn extract_printable(content: &str) -> String {
    content
        .chars()
        .filter(|c| c.is_ascii_graphic() || c.is_whitespace())
        .collect()
}
