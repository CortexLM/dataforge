use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedContent {
    pub raw: String,
    pub lines: Vec<String>,
    pub metadata: ContentMetadata,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMetadata {
    pub line_count: usize,
    pub char_count: usize,
    pub word_count: usize,
    pub has_headers: bool,
    pub encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub name: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub attributes: HashMap<String, String>,
}

pub fn parse_content(content: &str) -> Result<ParsedContent> {
    let lines: Vec<String> = content.lines().map(String::from).collect();
    let line_count = lines.len();
    let char_count = content.len();
    let word_count = count_words(content);
    let has_headers = detect_headers(&lines);
    
    let sections = extract_sections(&lines);
    
    Ok(ParsedContent {
        raw: content.to_string(),
        lines,
        metadata: ContentMetadata {
            line_count,
            char_count,
            word_count,
            has_headers,
            encoding: "utf-8".to_string(),
        },
        sections,
    })
}

fn count_words(content: &str) -> usize {
    content.split_whitespace().count()
}

fn detect_headers(lines: &[String]) -> bool {
    lines.iter().any(|line| {
        line.starts_with('#') || 
        line.starts_with("==") ||
        (line.len() > 0 && line.chars().all(|c| c == '=' || c == '-'))
    })
}

fn extract_sections(lines: &[String]) -> Vec<Section> {
    let mut sections = Vec::new();
    let mut current_section: Option<(String, usize, Vec<String>)> = None;
    
    for (idx, line) in lines.iter().enumerate() {
        if line.starts_with('#') || line.starts_with("==") {
            if let Some((name, start, content_lines)) = current_section.take() {
                sections.push(Section {
                    name,
                    start_line: start,
                    end_line: idx.saturating_sub(1),
                    content: content_lines.join("\n"),
                    attributes: HashMap::new(),
                });
            }
            
            let section_name = line.trim_start_matches('#').trim().to_string();
            current_section = Some((section_name, idx, Vec::new()));
        } else if let Some((_, _, ref mut content_lines)) = current_section {
            content_lines.push(line.clone());
        }
    }
    
    if let Some((name, start, content_lines)) = current_section {
        sections.push(Section {
            name,
            start_line: start,
            end_line: lines.len().saturating_sub(1),
            content: content_lines.join("\n"),
            attributes: HashMap::new(),
        });
    }
    
    if sections.is_empty() {
        sections.push(Section {
            name: "main".to_string(),
            start_line: 0,
            end_line: lines.len().saturating_sub(1),
            content: lines.join("\n"),
            attributes: HashMap::new(),
        });
    }
    
    sections
}

pub fn parse_key_value_pairs(content: &str) -> HashMap<String, String> {
    let mut pairs = HashMap::new();
    
    for line in content.lines() {
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim().to_string();
            let value = line[eq_pos + 1..].trim().to_string();
            pairs.insert(key, value);
        }
    }
    
    pairs
}

pub fn calculate_line_offset(content: &str, line_number: usize) -> usize {
    let mut offset = 0;
    let mut current_line = 0;
    
    for ch in content.chars() {
        if current_line >= line_number {
            break;
        }
        offset += 1;
        if ch == '\n' {
            current_line += 1;
        }
    }
    
    offset
}

pub fn merge_sections(sections: &[Section]) -> String {
    let mut result = String::new();
    
    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            result.push_str("\n\n");
        }
        result.push_str(&format!("# {}\n", section.name));
        result.push_str(&section.content);
    }
    
    result
}

pub struct ContentIterator<'a> {
    content: &'a str,
    position: usize,
    chunk_size: usize,
}

impl<'a> ContentIterator<'a> {
    pub fn new(content: &'a str, chunk_size: usize) -> Self {
        ContentIterator {
            content,
            position: 0,
            chunk_size,
        }
    }
}

impl<'a> Iterator for ContentIterator<'a> {
    type Item = &'a str;
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.content.len() {
            return None;
        }
        
        let end = std::cmp::min(self.position + self.chunk_size, self.content.len());
        let chunk = &self.content[self.position..end];
        self.position = end;
        Some(chunk)
    }
}
