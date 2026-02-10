use anyhow::{Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },
    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },
    #[error("Directory creation failed: {0}")]
    DirectoryCreationFailed(String),
    #[error("IO error accessing {path}: {details}")]
    IoError { path: String, details: String },
}

pub struct FileHandler {
    base_dir: PathBuf,
    temp_files: Vec<PathBuf>,
}

impl FileHandler {
    pub fn new(base_dir: &Path) -> Self {
        FileHandler {
            base_dir: base_dir.to_path_buf(),
            temp_files: Vec::new(),
        }
    }

    pub fn read_file(&self, path: &Path) -> Result<String> {
        let full_path = self.resolve_path(path);
        
        let mut file = File::open(&full_path)
            .with_context(|| format!("Cannot open file: {}", full_path.display()))?;
        
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        
        Ok(content)
    }

    pub fn write_file(&self, path: &Path, content: &str) -> Result<()> {
        let full_path = self.resolve_path(path);
        
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let mut file = File::create(&full_path)
            .with_context(|| format!("Cannot create file: {}", full_path.display()))?;
        
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        
        Ok(())
    }

    pub fn append_file(&self, path: &Path, content: &str) -> Result<()> {
        let full_path = self.resolve_path(path);
        
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&full_path)?;
        
        file.write_all(content.as_bytes())?;
        
        Ok(())
    }

    pub fn copy_file(&self, src: &Path, dst: &Path) -> Result<()> {
        let src_path = self.resolve_path(src);
        let dst_path = self.resolve_path(dst);
        
        if !src_path.exists() {
            return Err(StorageError::FileNotFound {
                path: src_path.display().to_string(),
            }
            .into());
        }
        
        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::copy(&src_path, &dst_path)?;
        
        Ok(())
    }

    pub fn safe_write(&self, path: &Path, content: &str) -> Result<()> {
        let full_path = self.resolve_path(path);
        
        if full_path.exists() {
            let temp_path = full_path.with_extension("tmp");
            
            fs::write(&temp_path, content)?;
            
            fs::rename(&temp_path, &full_path)?;
        } else {
            self.write_file(path, content)?;
        }
        
        Ok(())
    }

    pub fn delete_if_exists(&self, path: &Path) -> Result<bool> {
        let full_path = self.resolve_path(path);
        
        if full_path.exists() {
            fs::remove_file(&full_path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn list_files(&self, dir: &Path, extension: Option<&str>) -> Result<Vec<PathBuf>> {
        let full_path = self.resolve_path(dir);
        let mut files = Vec::new();
        
        if !full_path.is_dir() {
            return Ok(files);
        }
        
        for entry in fs::read_dir(&full_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                match extension {
                    Some(ext) => {
                        if path.extension().map(|e| e == ext).unwrap_or(false) {
                            files.push(path);
                        }
                    }
                    None => files.push(path),
                }
            }
        }
        
        Ok(files)
    }

    pub fn get_file_size(&self, path: &Path) -> Result<u64> {
        let full_path = self.resolve_path(path);
        let metadata = fs::metadata(&full_path)?;
        Ok(metadata.len())
    }

    pub fn ensure_directory(&self, path: &Path) -> Result<()> {
        let full_path = self.resolve_path(path);
        fs::create_dir_all(&full_path)?;
        Ok(())
    }

    fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_dir.join(path)
        }
    }

    pub fn create_temp_file(&mut self, prefix: &str) -> Result<PathBuf> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        
        let filename = format!("{}_{}.tmp", prefix, timestamp);
        let temp_path = self.base_dir.join(&filename);
        
        File::create(&temp_path)?;
        self.temp_files.push(temp_path.clone());
        
        Ok(temp_path)
    }

    pub fn process_file_with_backup(&self, path: &Path, processor: impl Fn(&str) -> String) -> Result<()> {
        let full_path = self.resolve_path(path);
        let backup_path = full_path.with_extension("bak");
        
        let content = self.read_file(path)?;
        
        fs::copy(&full_path, &backup_path)?;
        
        let processed = processor(&content);
        
        self.write_file(path, &processed)?;
        
        Ok(())
    }

    pub fn atomic_write(&self, path: &Path, content: &str) -> Result<()> {
        let full_path = self.resolve_path(path);
        let temp_path = full_path.with_extension("tmp.atomic");
        
        let mut file = File::create(&temp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        
        fs::rename(&temp_path, &full_path)?;
        
        Ok(())
    }

    pub fn read_binary(&self, path: &Path) -> Result<Vec<u8>> {
        let full_path = self.resolve_path(path);
        let data = fs::read(&full_path)?;
        Ok(data)
    }

    pub fn write_binary(&self, path: &Path, data: &[u8]) -> Result<()> {
        let full_path = self.resolve_path(path);
        
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::write(&full_path, data)?;
        Ok(())
    }
}

impl Drop for FileHandler {
    fn drop(&mut self) {
        for temp_file in &self.temp_files {
            let _ = fs::remove_file(temp_file);
        }
    }
}

pub fn batch_process_files(
    paths: &[PathBuf],
    handler: &FileHandler,
    processor: impl Fn(&str) -> String,
) -> Vec<Result<String>> {
    paths
        .iter()
        .map(|path| {
            let content = handler.read_file(path)?;
            Ok(processor(&content))
        })
        .collect()
}
