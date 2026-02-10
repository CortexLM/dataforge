//! Workspace generation module for synthetic benchmark workspaces.
//!
//! This module provides the infrastructure for generating complete code workspaces
//! with deliberately injected vulnerabilities and bugs for security benchmarking.
//!
//! # Overview
//!
//! The workspace generation system creates realistic codebases that contain
//! security vulnerabilities or bugs that agents must identify and fix.
//! Each workspace includes:
//!
//! - Source code files with injected issues
//! - Build/configuration files
//! - Test files
//! - Verification scripts to check if fixes were applied correctly
//!
//! # Architecture
//!
//! The module consists of several components:
//!
//! - **Types** (`types`): Core data structures for workspace generation
//! - **Generator** (`generator`): Main workspace generation logic using LLM
//! - **Exporter** (`exporter`): Export workspaces to .zip files or directories
//! - **Cleaner** (`cleaner`): Remove hints and comments from generated code
//!
//! # Example
//!
//! ```ignore
//! use dataforge::workspace::{
//!     WorkspaceGenerator, WorkspaceSpec, WorkspaceLanguage,
//!     VulnerabilityType, WorkspaceExporter,
//! };
//! use dataforge::llm::LiteLlmClient;
//! use std::sync::Arc;
//!
//! // Set up LLM client
//! let llm = Arc::new(LiteLlmClient::from_env()?);
//!
//! // Define workspace specification
//! let spec = WorkspaceSpec::new("sql-injection-fix")
//!     .with_name("SQL Injection Fix Challenge")
//!     .with_language(WorkspaceLanguage::Python)
//!     .with_vulnerability(VulnerabilityType::SqlInjection)
//!     .with_project_type("web-api")
//!     .with_difficulty(7);
//!
//! // Generate workspace
//! let generator = WorkspaceGenerator::new(llm);
//! let workspace = generator.generate(&spec).await?;
//!
//! // Export to zip
//! let exporter = WorkspaceExporter::new();
//! exporter.export_to_zip(&workspace, Path::new("output/workspace.zip")).await?;
//! ```
//!
//! # Supported Languages
//!
//! The system supports multiple programming languages:
//!
//! - Python (Flask, Django, FastAPI)
//! - JavaScript/TypeScript (Node.js, Express)
//! - Rust
//! - Go
//! - Java
//! - C/C++
//! - Ruby
//! - PHP
//!
//! # Vulnerability Types
//!
//! Common security vulnerabilities that can be injected:
//!
//! - SQL Injection (CWE-89)
//! - Cross-Site Scripting / XSS (CWE-79)
//! - Authentication Bypass (CWE-287)
//! - Path Traversal (CWE-22)
//! - Command Injection (CWE-78)
//! - Insecure Deserialization (CWE-502)
//! - Server-Side Request Forgery / SSRF (CWE-918)
//! - Race Conditions (CWE-362)
//! - Memory Leaks (CWE-401)
//! - Buffer Overflows (CWE-120)
//!
//! See [`VulnerabilityType`] for the complete list.

pub mod cleaner;
pub mod exporter;
pub mod generator;
pub mod types;

// Re-export core types
pub use types::{
    GeneratedWorkspace, InjectedVulnerability, ScriptType, VerificationScript, VulnerabilityType,
    WorkspaceFile, WorkspaceFileType, WorkspaceLanguage, WorkspaceSpec,
};

// Re-export generator types
pub use generator::{GeneratorConfig, WorkspaceGen, WorkspaceGenerator, WorkspaceGeneratorBuilder};

// Re-export exporter types
pub use exporter::{
    default_exclude_patterns, ExportConfig, WorkspaceExportResult, WorkspaceExporter,
    WorkspaceExporterBuilder,
};

// Re-export cleaner types
pub use cleaner::{
    default_hint_patterns, default_security_terms, CleanerConfig, CleaningResult, FileModification,
    WorkspaceCleaner, WorkspaceCleanerBuilder,
};
