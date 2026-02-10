pub mod parser;
pub mod transformer;

use crate::config::{Config, ProcessingMode};
use crate::storage::FileHandler;
use anyhow::Result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessorError {
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Transform error: {0}")]
    TransformError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("File too large: {size} bytes (max: {max})")]
    FileTooLarge { size: usize, max: usize },
}

pub struct Processor {
    config: Config,
    pub handler: FileHandler,
    transformer: transformer::Transformer,
}

impl Processor {
    pub fn new(config: Config, handler: FileHandler) -> Self {
        let transformer = transformer::Transformer::new(&config);
        Processor {
            config,
            handler,
            transformer,
        }
    }

    pub fn process(&self, content: &str) -> Result<String> {
        let size = content.len();
        if size > self.config.max_file_size {
            return Err(ProcessorError::FileTooLarge {
                size,
                max: self.config.max_file_size,
            }
            .into());
        }

        match self.config.mode {
            ProcessingMode::Transform => self.process_transform(content),
            ProcessingMode::Validate => self.process_validate(content),
            ProcessingMode::Hash => self.process_hash(content),
        }
    }

    fn process_transform(&self, content: &str) -> Result<String> {
        let parsed = parser::parse_content(content)?;
        let transformed = self.transformer.apply_transforms(&parsed)?;
        Ok(transformed)
    }

    fn process_validate(&self, content: &str) -> Result<String> {
        let parsed = parser::parse_content(content)?;
        let validation_result = crate::utils::validation::validate_content(&parsed);
        
        match validation_result {
            Ok(_) => Ok("Validation passed".to_string()),
            Err(e) => Ok(format!("Validation failed: {}", e)),
        }
    }

    fn process_hash(&self, content: &str) -> Result<String> {
        let hash = crate::utils::crypto::hash_data(content.as_bytes());
        Ok(format!("SHA256: {}", hash))
    }
}

pub struct BatchProcessor {
    processors: Vec<Processor>,
    pending_items: Vec<String>,
}

impl BatchProcessor {
    pub fn new(config: Config, worker_count: usize) -> Self {
        let mut processors = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let handler = FileHandler::new(&config.output_dir);
            processors.push(Processor::new(config.clone(), handler));
        }
        BatchProcessor {
            processors,
            pending_items: Vec::new(),
        }
    }

    pub fn add_item(&mut self, item: String) {
        self.pending_items.push(item);
    }

    pub fn process_all(&self) -> Vec<Result<String>> {
        let mut results = Vec::new();
        for (idx, item) in self.pending_items.iter().enumerate() {
            let processor_idx = idx % self.processors.len();
            results.push(self.processors[processor_idx].process(item));
        }
        results
    }

    pub fn get_stats(&self) -> ProcessingStats {
        let total = self.pending_items.len();
        let total_size: usize = self.pending_items.iter().map(|s| s.len()).sum();
        ProcessingStats {
            total_items: total,
            total_bytes: total_size,
            average_size: if total > 0 { total_size / total } else { 0 },
        }
    }
}

#[derive(Debug)]
pub struct ProcessingStats {
    pub total_items: usize,
    pub total_bytes: usize,
    pub average_size: usize,
}
