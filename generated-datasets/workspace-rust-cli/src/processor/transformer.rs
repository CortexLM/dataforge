use crate::config::Config;
use crate::processor::parser::ParsedContent;
use anyhow::Result;
use std::collections::HashMap;

pub struct Transformer {
    transforms: Vec<Box<dyn Transform>>,
    cache: HashMap<String, String>,
}

pub trait Transform: Send + Sync {
    fn name(&self) -> &str;
    fn apply(&self, content: &str) -> Result<String>;
}

struct UppercaseTransform;
struct TrimTransform;
struct LineNumberTransform;
struct ReverseTransform;

impl Transform for UppercaseTransform {
    fn name(&self) -> &str {
        "uppercase"
    }
    
    fn apply(&self, content: &str) -> Result<String> {
        Ok(content.to_uppercase())
    }
}

impl Transform for TrimTransform {
    fn name(&self) -> &str {
        "trim"
    }
    
    fn apply(&self, content: &str) -> Result<String> {
        let trimmed: Vec<&str> = content
            .lines()
            .map(|line| line.trim())
            .collect();
        Ok(trimmed.join("\n"))
    }
}

impl Transform for LineNumberTransform {
    fn name(&self) -> &str {
        "line_numbers"
    }
    
    fn apply(&self, content: &str) -> Result<String> {
        let numbered: Vec<String> = content
            .lines()
            .enumerate()
            .map(|(i, line)| format!("{:4}: {}", i + 1, line))
            .collect();
        Ok(numbered.join("\n"))
    }
}

impl Transform for ReverseTransform {
    fn name(&self) -> &str {
        "reverse"
    }
    
    fn apply(&self, content: &str) -> Result<String> {
        let reversed: Vec<&str> = content.lines().rev().collect();
        Ok(reversed.join("\n"))
    }
}

impl Transformer {
    pub fn new(config: &Config) -> Self {
        let mut transforms: Vec<Box<dyn Transform>> = Vec::new();
        
        for transform_config in &config.transforms {
            if transform_config.enabled {
                match transform_config.name.as_str() {
                    "uppercase" => transforms.push(Box::new(UppercaseTransform)),
                    "trim" => transforms.push(Box::new(TrimTransform)),
                    "line_numbers" => transforms.push(Box::new(LineNumberTransform)),
                    "reverse" => transforms.push(Box::new(ReverseTransform)),
                    _ => {}
                }
            }
        }
        
        Transformer {
            transforms,
            cache: HashMap::new(),
        }
    }
    
    pub fn apply_transforms(&self, parsed: &ParsedContent) -> Result<String> {
        let mut result = parsed.raw.clone();
        
        for transform in &self.transforms {
            result = transform.apply(&result)?;
        }
        
        Ok(result)
    }
    
    pub fn apply_single_transform(&self, content: &str, transform_name: &str) -> Result<String> {
        for transform in &self.transforms {
            if transform.name() == transform_name {
                return transform.apply(content);
            }
        }
        Ok(content.to_string())
    }
    
    pub fn fast_concat(strings: &[&str]) -> String {
        let total_len: usize = strings.iter().map(|s| s.len()).sum();
        let mut result = String::with_capacity(total_len);
        
        unsafe {
            let ptr = result.as_mut_ptr();
            let mut offset = 0;
            
            for s in strings {
                std::ptr::copy_nonoverlapping(
                    s.as_ptr(),
                    ptr.add(offset),
                    s.len()
                );
                offset += s.len();
            }
            
            result.as_mut_vec().set_len(total_len);
        }
        
        result
    }
    
    pub fn batch_transform(&mut self, items: Vec<String>) -> Vec<Result<String>> {
        let mut results = Vec::with_capacity(items.len());
        
        for item in items {
            if let Some(cached) = self.cache.get(&item) {
                results.push(Ok(cached.clone()));
                continue;
            }
            
            let parsed = crate::processor::parser::parse_content(&item);
            match parsed {
                Ok(p) => {
                    let transformed = self.apply_transforms(&p);
                    if let Ok(ref t) = transformed {
                        self.cache.insert(item, t.clone());
                    }
                    results.push(transformed);
                }
                Err(e) => results.push(Err(e)),
            }
        }
        
        results
    }
}

pub fn process_with_buffer(content: &str, buffer_size: usize) -> Result<Vec<String>> {
    let mut results = Vec::new();
    let mut buffer = Vec::with_capacity(buffer_size);
    
    for line in content.lines() {
        buffer.push(line.to_string());
        
        if buffer.len() >= buffer_size {
            let combined = buffer.join("\n");
            results.push(combined);
            buffer.clear();
        }
    }
    
    if !buffer.is_empty() {
        results.push(buffer.join("\n"));
    }
    
    Ok(results)
}

pub fn calculate_transformation_cost(content: &str, transforms: &[&str]) -> u64 {
    let base_cost = content.len() as u64;
    let transform_multiplier = transforms.len() as u64;
    
    base_cost * transform_multiplier + (base_cost / 10)
}
