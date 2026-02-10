pub mod cli;
pub mod config;
pub mod processor;
pub mod storage;
pub mod utils;

use anyhow::Result;
use config::Config;
use processor::Processor;
use storage::FileHandler;

/// Main entry point for the file processor library
pub async fn run_processor(config: Config) -> Result<()> {
    let handler = FileHandler::new(&config.output_dir);
    let processor = Processor::new(config.clone(), handler);
    
    for input_path in &config.input_files {
        let content = std::fs::read_to_string(input_path)?;
        let processed = processor.process(&content)?;
        
        let output_file = config.output_dir.join(
            input_path.file_name().unwrap()
        );
        
        processor.handler.write_file(&output_file, &processed)?;
        
        if config.verbose {
            println!("Processed: {} -> {}", input_path.display(), output_file.display());
        }
    }
    
    Ok(())
}

/// Calculate checksum for data integrity
pub fn calculate_checksum(data: &[u8]) -> String {
    utils::crypto::hash_data(data)
}
