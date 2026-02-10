//! Project templates for synthetic workspace generation.
//!
//! This module provides templates for different project types that serve as
//! starting points for code generation.

use serde::{Deserialize, Serialize};

use super::config::{LanguageTarget, ProjectCategory};

/// A template for generating a workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTemplate {
    /// Template identifier.
    pub id: String,
    /// Template name.
    pub name: String,
    /// Target language.
    pub language: LanguageTarget,
    /// Project category.
    pub category: ProjectCategory,
    /// Template description.
    pub description: String,
    /// Framework to use.
    pub framework: Option<String>,
    /// Directory structure.
    pub directories: Vec<String>,
    /// Files to generate with their purposes.
    pub files: Vec<FileTemplate>,
    /// Dependencies to include.
    pub dependencies: Vec<DependencyTemplate>,
    /// Common vulnerability patterns for this template.
    pub vulnerability_patterns: Vec<VulnerabilityPattern>,
}

impl WorkspaceTemplate {
    /// Creates a new workspace template.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        language: LanguageTarget,
        category: ProjectCategory,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            language,
            category,
            description: String::new(),
            framework: None,
            directories: Vec::new(),
            files: Vec::new(),
            dependencies: Vec::new(),
            vulnerability_patterns: Vec::new(),
        }
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the framework.
    pub fn with_framework(mut self, framework: impl Into<String>) -> Self {
        self.framework = Some(framework.into());
        self
    }

    /// Adds directories.
    pub fn with_directories<I, S>(mut self, dirs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.directories.extend(dirs.into_iter().map(Into::into));
        self
    }

    /// Adds file templates.
    pub fn with_files(mut self, files: Vec<FileTemplate>) -> Self {
        self.files = files;
        self
    }

    /// Adds dependencies.
    pub fn with_dependencies(mut self, deps: Vec<DependencyTemplate>) -> Self {
        self.dependencies = deps;
        self
    }

    /// Adds vulnerability patterns.
    pub fn with_vulnerability_patterns(mut self, patterns: Vec<VulnerabilityPattern>) -> Self {
        self.vulnerability_patterns = patterns;
        self
    }

    // =========================================================================
    // Built-in Templates
    // =========================================================================

    /// Python Flask REST API template - Comprehensive enterprise scale with substantial code per file.
    pub fn python_flask_api() -> Self {
        Self::new("python-flask-api", "Flask REST API", LanguageTarget::Python, ProjectCategory::WebApi)
            .with_description("A comprehensive enterprise-grade RESTful API built with Flask, featuring user authentication with JWT and OAuth2, RBAC with multi-level permissions, PostgreSQL database with advanced queries, Redis caching, Celery task queues, file storage with S3 integration, email notifications with templates, audit logging, rate limiting, and comprehensive monitoring. Each file should contain 150-300 lines of production-quality code with proper error handling, logging, and documentation.")
            .with_framework("Flask")
            .with_directories(vec![
                "app",
                "app/routes",
                "app/routes/api",
                "app/routes/api/v1",
                "app/models",
                "app/services",
                "app/utils",
                "app/middleware",
                "tests",
                "tests/unit",
                "tests/integration",
            ])
            .with_files(vec![
                // Core application files (each 150-300 lines)
                FileTemplate::source("app/__init__.py", "Flask application factory with full extensions initialization, error handlers, request hooks, and logging setup. Include app context management, health checks, and graceful shutdown handling.", true),
                FileTemplate::source("app/extensions.py", "Flask extensions initialization: SQLAlchemy with connection pooling, Flask-Migrate, Flask-CORS with configurable origins, Flask-JWT-Extended with custom token callbacks, Flask-Limiter for rate limiting, Flask-Mail for emails, and Celery setup.", false),

                // Route files - API v1 (each 200-400 lines with full CRUD, validation, error handling)
                FileTemplate::source("app/routes/__init__.py", "Blueprint registration with error handlers and request/response logging middleware", false),
                FileTemplate::source("app/routes/api/v1/__init__.py", "V1 API blueprint with version-specific middleware, CORS configuration, and route registration", false),
                FileTemplate::source("app/routes/api/v1/auth.py", "Complete authentication endpoints: login with username/email, logout, token refresh, password reset flow (request, verify, reset), email verification, OAuth2 callbacks (Google, GitHub), 2FA setup/verify/disable, session management, and account lockout handling. Include request validation, rate limiting decorators, and comprehensive error responses.", true),
                FileTemplate::source("app/routes/api/v1/users.py", "Full user management CRUD: list users with pagination/filtering/sorting, get user by ID with relations, create user with validation, update user profile and password, soft delete and hard delete, user search, bulk operations, user export to CSV/JSON, and user statistics. Include role-based access control decorators.", true),
                FileTemplate::source("app/routes/api/v1/roles.py", "Role management: list roles with permissions, create/update/delete roles, assign/revoke roles to users, role hierarchy management, permission sets, and audit logging for role changes. Include proper authorization checks.", true),
                FileTemplate::source("app/routes/api/v1/files.py", "File management: upload with multipart support and progress tracking, download with range requests and streaming, list files with filtering, delete files, file metadata update, virus scanning integration, file type validation, S3 presigned URLs, and file sharing with expiring links.", true),
                FileTemplate::source("app/routes/api/v1/notifications.py", "Notification system: list notifications with read/unread status, mark as read, bulk mark read, notification preferences, push notification registration, email notification triggers, SMS integration stubs, and notification templates.", true),
                FileTemplate::source("app/routes/api/v1/webhooks.py", "Webhook management: register webhooks with URL validation, list/update/delete webhooks, webhook event types, delivery logs, retry failed deliveries, webhook secret rotation, and signature verification helpers.", true),

                // Model files (each 100-200 lines with relationships, validations, methods)
                FileTemplate::source("app/models/__init__.py", "Model exports and common utilities like pagination mixin and soft delete mixin", false),
                FileTemplate::source("app/models/base.py", "Base model with ID, timestamps, soft delete, audit fields, common query methods, and serialization helpers", false),
                FileTemplate::source("app/models/user.py", "User model with all profile fields, relationships to roles/sessions/files, password hashing methods, email verification, 2FA fields, OAuth providers, account status, login tracking, and custom query methods for user search.", true),
                FileTemplate::source("app/models/role.py", "Role model with permission relationships, role hierarchy, role assignments, and methods for checking permissions and inheriting permissions from parent roles.", true),
                FileTemplate::source("app/models/session.py", "Session model with device information, IP tracking, refresh token storage, expiration handling, session revocation, and concurrent session limits.", true),
                FileTemplate::source("app/models/file.py", "File model with metadata fields, storage backend info, file versions, access logs, sharing settings, and methods for generating presigned URLs and managing file lifecycle.", true),
                FileTemplate::source("app/models/audit_log.py", "Audit log model with user reference, action type, resource type, old/new values, IP address, user agent, and query methods for audit trail retrieval.", false),
                FileTemplate::source("app/models/webhook.py", "Webhook model with URL, events, secret, status, delivery logs relationship, retry configuration, and methods for triggering and verifying webhooks.", true),

                // Service layer (each 200-400 lines with business logic)
                FileTemplate::source("app/services/__init__.py", "Service exports and base service class with common patterns", false),
                FileTemplate::source("app/services/auth.py", "Authentication service: login validation, token generation with claims, token refresh logic, password reset flow, email verification, OAuth2 token exchange, 2FA validation with TOTP, session management, and account lockout logic.", true),
                FileTemplate::source("app/services/user.py", "User service: CRUD operations with validation, user search with full-text, profile updates, password changes, role assignment, user import/export, statistics calculation, and cleanup of inactive users.", true),
                FileTemplate::source("app/services/file.py", "File service: upload handling with chunking, storage backend abstraction (local, S3), file type validation, virus scanning stub, thumbnail generation, file versioning, and cleanup of orphaned files.", true),
                FileTemplate::source("app/services/email.py", "Email service: template rendering with Jinja2, SMTP sending with retry logic, email queue integration, HTML/text multipart, attachments, email verification tokens, and password reset emails.", true),
                FileTemplate::source("app/services/notification.py", "Notification service: create notifications, bulk notifications, push notification sending (Firebase stub), email notification trigger, notification preferences check, and notification cleanup.", true),
                FileTemplate::source("app/services/webhook.py", "Webhook service: event dispatching, delivery with retry and exponential backoff, signature generation, delivery status tracking, and failed webhook reprocessing.", true),

                // Utility files (each 150-300 lines)
                FileTemplate::source("app/utils/__init__.py", "Utility exports", false),
                FileTemplate::source("app/utils/crypto.py", "Cryptographic utilities: password hashing with bcrypt, token generation with secrets, encryption/decryption with Fernet, HMAC signature generation, TOTP generation and verification, and secure random string generation.", true),
                FileTemplate::source("app/utils/validation.py", "Input validation: email validation with DNS check, password strength validation, phone number validation, URL validation, file type validation, SQL injection prevention, XSS sanitization, and custom validators.", true),
                FileTemplate::source("app/utils/decorators.py", "Custom decorators: require_auth with role check, rate_limit with custom keys, cache_response with TTL, audit_log decorator, validate_json with schema, and require_permissions.", true),
                FileTemplate::source("app/utils/helpers.py", "Helper functions: pagination helpers, response formatting, error response builder, date/time utilities, string utilities, IP address utilities, and JSON serialization helpers.", false),
                FileTemplate::source("app/utils/file_handler.py", "File handling: secure filename generation, MIME type detection, file size validation, archive extraction, image processing stubs, and temporary file management.", true),

                // Middleware (each 100-200 lines)
                FileTemplate::source("app/middleware/__init__.py", "Middleware exports and registration", false),
                FileTemplate::source("app/middleware/auth.py", "Authentication middleware: JWT validation, token refresh logic, user loading, permission checking, and request context population.", true),
                FileTemplate::source("app/middleware/rate_limit.py", "Rate limiting middleware: Redis-based rate limiting, per-endpoint limits, user-based limits, IP-based limits, and rate limit headers.", true),
                FileTemplate::source("app/middleware/security.py", "Security middleware: CORS headers, CSP headers, XSS protection, clickjacking protection, and request sanitization.", true),

                // Database
                FileTemplate::source("app/database.py", "Database utilities: connection management, session handling, query logging, transaction helpers, and database health check.", true),

                // Configuration
                FileTemplate::config("config.py", "Configuration with environment support, validation, secrets loading, and default values for all settings.", true),
                FileTemplate::config("requirements.txt", "Python dependencies with version pinning", false),

                // Tests (each 200-400 lines)
                FileTemplate::test("tests/__init__.py", "Test package init", false),
                FileTemplate::test("tests/conftest.py", "Pytest fixtures: app fixture, client fixture, database fixtures, user fixtures, mock fixtures, and cleanup.", false),
                FileTemplate::test("tests/unit/test_auth_service.py", "Auth service unit tests: login, logout, token refresh, password reset, 2FA", false),
                FileTemplate::test("tests/unit/test_user_service.py", "User service unit tests: CRUD, search, validation", false),
                FileTemplate::test("tests/integration/test_auth_endpoints.py", "Auth endpoint integration tests with full flow", false),
                FileTemplate::test("tests/integration/test_user_endpoints.py", "User endpoint integration tests", false),
            ])
            .with_dependencies(vec![
                DependencyTemplate::new("flask", ">=3.0.0"),
                DependencyTemplate::new("flask-cors", ">=4.0.0"),
                DependencyTemplate::new("pyjwt", ">=2.8.0"),
                DependencyTemplate::new("psycopg2-binary", ">=2.9.0"),
                DependencyTemplate::new("python-dotenv", ">=1.0.0"),
                DependencyTemplate::dev("pytest", ">=7.0.0"),
            ])
            .with_vulnerability_patterns(vec![
                VulnerabilityPattern::new("sql_injection")
                    .in_file("app/database.py")
                    .in_file("app/routes/users.py")
                    .with_description("Direct string interpolation in SQL queries"),
                VulnerabilityPattern::new("authentication_bypass")
                    .in_file("app/routes/auth.py")
                    .with_description("Missing or weak authentication checks"),
                VulnerabilityPattern::new("insecure_deserialization")
                    .in_file("app/routes/auth.py")
                    .in_file("app/utils/crypto.py")
                    .with_description("Using pickle or yaml.load without safe_load"),
                VulnerabilityPattern::new("weak_cryptography")
                    .in_file("app/utils/crypto.py")
                    .with_description("Using MD5/SHA1 for passwords, weak random"),
                VulnerabilityPattern::new("path_traversal")
                    .in_file("app/routes/files.py")
                    .with_description("Unchecked file path from user input"),
                VulnerabilityPattern::new("hardcoded_secrets")
                    .in_file("config.py")
                    .with_description("API keys or passwords in code"),
            ])
    }

    /// Python FastAPI microservice template.
    pub fn python_fastapi_service() -> Self {
        Self::new("python-fastapi-service", "FastAPI Microservice", LanguageTarget::Python, ProjectCategory::Microservice)
            .with_description("A microservice built with FastAPI, featuring async operations, data validation, and external API calls")
            .with_framework("FastAPI")
            .with_directories(vec![
                "app",
                "app/api",
                "app/api/v1",
                "app/core",
                "app/db",
                "app/schemas",
                "app/services",
                "tests",
            ])
            .with_files(vec![
                FileTemplate::source("app/main.py", "FastAPI application entry", true),
                FileTemplate::source("app/api/v1/endpoints.py", "API endpoints", true),
                FileTemplate::source("app/core/config.py", "Configuration", true),
                FileTemplate::source("app/core/security.py", "Security utilities", true),
                FileTemplate::source("app/db/database.py", "Database setup", true),
                FileTemplate::source("app/db/crud.py", "CRUD operations", true),
                FileTemplate::source("app/schemas/user.py", "Pydantic schemas", false),
                FileTemplate::source("app/services/external.py", "External API calls", true),
                FileTemplate::config("requirements.txt", "Dependencies", false),
            ])
            .with_vulnerability_patterns(vec![
                VulnerabilityPattern::new("ssrf")
                    .in_file("app/services/external.py")
                    .with_description("Unvalidated URL in external requests"),
                VulnerabilityPattern::new("sql_injection")
                    .in_file("app/db/crud.py")
                    .with_description("Raw SQL with user input"),
                VulnerabilityPattern::new("mass_assignment")
                    .in_file("app/api/v1/endpoints.py")
                    .with_description("Accepting all fields without filtering"),
            ])
    }

    /// Node.js Express API template.
    pub fn nodejs_express_api() -> Self {
        Self::new(
            "nodejs-express-api",
            "Express.js REST API",
            LanguageTarget::JavaScript,
            ProjectCategory::WebApi,
        )
        .with_description(
            "A RESTful API built with Express.js, featuring JWT auth, MongoDB, and file handling",
        )
        .with_framework("Express")
        .with_directories(vec![
            "src",
            "src/routes",
            "src/controllers",
            "src/models",
            "src/middleware",
            "src/utils",
            "src/config",
            "tests",
        ])
        .with_files(vec![
            FileTemplate::source("src/index.js", "Application entry", true),
            FileTemplate::source("src/app.js", "Express setup", true),
            FileTemplate::source("src/routes/auth.js", "Auth routes", true),
            FileTemplate::source("src/routes/users.js", "User routes", true),
            FileTemplate::source("src/controllers/authController.js", "Auth logic", true),
            FileTemplate::source("src/controllers/userController.js", "User logic", true),
            FileTemplate::source("src/models/User.js", "User model", false),
            FileTemplate::source("src/middleware/auth.js", "Auth middleware", true),
            FileTemplate::source("src/middleware/validate.js", "Validation", true),
            FileTemplate::source("src/utils/crypto.js", "Crypto utils", true),
            FileTemplate::source("src/utils/db.js", "Database utils", true),
            FileTemplate::source("src/config/index.js", "Configuration", true),
            FileTemplate::config("package.json", "NPM config", false),
            FileTemplate::test("tests/auth.test.js", "Auth tests", false),
        ])
        .with_vulnerability_patterns(vec![
            VulnerabilityPattern::new("nosql_injection")
                .in_file("src/controllers/userController.js")
                .with_description("Unsanitized input in MongoDB queries"),
            VulnerabilityPattern::new("xss")
                .in_file("src/routes/users.js")
                .with_description("Reflected user input without encoding"),
            VulnerabilityPattern::new("prototype_pollution")
                .in_file("src/utils/db.js")
                .with_description("Object merge without prototype check"),
            VulnerabilityPattern::new("jwt_weakness")
                .in_file("src/middleware/auth.js")
                .with_description("Algorithm confusion or weak secret"),
        ])
    }

    /// Rust CLI tool template - File processor with comprehensive functionality. Each file should be 150-300 lines.
    pub fn rust_cli_tool() -> Self {
        Self::new("rust-cli-tool", "Rust CLI Tool", LanguageTarget::Rust, ProjectCategory::CliTool)
            .with_description("A comprehensive command-line file processing tool built with Rust featuring: multi-format file processing (JSON, CSV, YAML, XML, binary), shell command execution with output capture, encryption/decryption with AES-256, file hashing with SHA256/MD5/Blake3, compression with gzip/zstd/brotli, regex-based file search, configuration management with TOML, and parallel processing with rayon. Each file should contain 150-300 lines of production-quality idiomatic Rust code with proper error handling using thiserror/anyhow, comprehensive logging with tracing, and full documentation.")
            .with_framework("clap")
            .with_directories(vec![
                "src",
                "src/commands",
                "src/config",
                "src/processors",
                "src/utils",
                "src/formatters",
                "tests",
            ])
            .with_files(vec![
                // Core application files (each 150-250 lines)
                FileTemplate::source("src/main.rs", "Entry point with clap CLI argument parsing including subcommands for process, exec, encrypt, hash, compress, search, and config. Include version info, logging setup with tracing-subscriber, error handling with exit codes, and signal handling for graceful shutdown.", true),
                FileTemplate::source("src/lib.rs", "Library root exposing public API for programmatic use, re-exports of key types, and prelude module", false),
                FileTemplate::source("src/error.rs", "Custom error types using thiserror: IoError, ConfigError, ProcessError, CryptoError, ParseError, NetworkError. Include error conversion impls and user-friendly error messages.", false),

                // Command modules (each 200-350 lines with full implementation)
                FileTemplate::source("src/commands/mod.rs", "Command module exports and CommandResult type alias", false),
                FileTemplate::source("src/commands/process.rs", "File processing command: read files in multiple formats (JSON, CSV, YAML, TOML, XML, binary), transform data (map, filter, sort, group), write to output format, support for stdin/stdout, recursive directory processing, and progress reporting. Include format auto-detection.", true),
                FileTemplate::source("src/commands/exec.rs", "Shell command execution: run arbitrary commands with full argument handling, capture stdout/stderr separately, timeout support, working directory control, environment variable injection, command chaining with pipes, and process exit code handling. Include shell expansion.", true),
                FileTemplate::source("src/commands/encrypt.rs", "Encryption/decryption command: AES-256-GCM encryption with password-derived keys using Argon2, file encryption with authentication, key file support, batch encryption, secure key generation, and encrypted file format with magic bytes and version.", true),
                FileTemplate::source("src/commands/hash.rs", "File hashing command: support for MD5, SHA1, SHA256, SHA512, Blake3 algorithms, recursive hashing, hash verification mode, batch processing with parallelism, output in various formats (hex, base64, BSD-style), and hash file generation/verification.", true),
                FileTemplate::source("src/commands/compress.rs", "Compression/decompression: support gzip, zstd, brotli, xz formats, compression level control, archive creation (tar.gz, zip), archive extraction with path traversal protection, streaming compression for large files, and multi-threaded compression.", true),
                FileTemplate::source("src/commands/search.rs", "File search with regex: recursive directory search, regex pattern matching, glob patterns for file filtering, context lines (before/after), output formatting (plain, JSON, match-only), replacement mode with backup, and parallel search with rayon.", true),
                FileTemplate::source("src/commands/config.rs", "Configuration management: get/set config values, list all settings, reset to defaults, config file location management, environment variable override, and config validation with schema.", false),

                // Configuration (each 150-200 lines)
                FileTemplate::source("src/config/mod.rs", "Config module exports and Config struct definition", false),
                FileTemplate::source("src/config/settings.rs", "Configuration settings: all CLI options as config fields, serialization/deserialization with serde, config file paths (user, system, local), config merging from multiple sources, and validation.", true),
                FileTemplate::source("src/config/loader.rs", "Config loader: load from TOML/JSON/YAML files, environment variables with prefix, command-line override, default values, config file search in standard locations, and error reporting for invalid config.", true),

                // File processors (each 200-300 lines with full parsing/serialization)
                FileTemplate::source("src/processors/mod.rs", "Processor trait definition and factory function", false),
                FileTemplate::source("src/processors/json.rs", "JSON processor: parse with serde_json, streaming for large files with json_stream, pretty printing, JSON path queries, schema validation stub, and merge/diff operations.", true),
                FileTemplate::source("src/processors/csv.rs", "CSV processor: parse with csv crate, header handling, delimiter configuration, quote handling, type inference for columns, CSV to JSON conversion, and aggregate operations (sum, count, avg).", true),
                FileTemplate::source("src/processors/yaml.rs", "YAML processor: parse with serde_yaml, multi-document support, anchor/alias handling, YAML to JSON conversion, and merge key handling.", true),
                FileTemplate::source("src/processors/binary.rs", "Binary processor: hex dump, binary to base64, file analysis (entropy, magic bytes), binary diff, and simple pattern search in binary files.", true),

                // Utilities (each 150-250 lines)
                FileTemplate::source("src/utils/mod.rs", "Utility module exports", false),
                FileTemplate::source("src/utils/file.rs", "File system utilities: path canonicalization, safe path joining, recursive directory operations, file copy with progress, atomic file writes, temporary file creation, file locking, and file type detection.", true),
                FileTemplate::source("src/utils/crypto.rs", "Cryptographic utilities: AES-256-GCM encrypt/decrypt, Argon2 key derivation, secure random bytes, HMAC-SHA256, constant-time comparison, and secure memory zeroing.", true),
                FileTemplate::source("src/utils/parallel.rs", "Parallel processing: rayon-based parallel iteration, work stealing queue, progress tracking for parallel operations, and concurrent file processing with controlled parallelism.", true),
                FileTemplate::source("src/utils/progress.rs", "Progress reporting: indicatif progress bars, spinner for indeterminate operations, multi-progress for parallel tasks, and quiet mode support.", false),

                // Additional command modules (each 200-350 lines)
                FileTemplate::source("src/commands/version.rs", "Version information command: display version, build info, git commit hash, build timestamp, feature flags enabled, dependency versions, system info (OS, arch), and update check with HTTP request to releases API.", false),
                FileTemplate::source("src/commands/init.rs", "Project initialization command: create config file with defaults, setup directory structure, create .gitignore with appropriate patterns, generate sample input files, validate existing setup, and interactive prompts for configuration.", true),
                FileTemplate::source("src/commands/clean.rs", "Cleanup operations command: remove temporary files, clear cache directories, cleanup orphaned lock files, prune old backups with age threshold, disk usage report, dry-run mode, and confirmation prompts.", true),
                FileTemplate::source("src/commands/watch.rs", "File watch mode: use notify crate for filesystem events, debouncing for rapid changes, glob pattern filtering, recursive directory watching, event callbacks for file operations, graceful shutdown on signal, and integration with process command.", true),

                // Output formatters (each 150-250 lines)
                FileTemplate::source("src/formatters/mod.rs", "Formatter trait definition, OutputFormat enum (Plain, Json, Table, Csv, Yaml), factory function to create formatters, and common formatting utilities.", false),
                FileTemplate::source("src/formatters/json.rs", "JSON output formatter: pretty printing with configurable indentation, streaming JSON output for large results, JSON Lines format support, partial output on error, and schema-aware formatting.", true),
                FileTemplate::source("src/formatters/table.rs", "Table output formatter: ASCII table rendering with borders, column alignment (left, right, center), automatic column width calculation, header styling, row striping, truncation with ellipsis, and unicode box drawing characters.", true),

                // Configuration files
                FileTemplate::config("Cargo.toml", "Cargo manifest with dependencies: clap, serde, serde_json, serde_yaml, csv, toml, rayon, indicatif, tracing, thiserror, anyhow, aes-gcm, argon2, sha2, md5, blake3, flate2, zstd, brotli, regex, walkdir, tempfile, notify", false),

                // Tests (each 150-300 lines)
                FileTemplate::test("tests/integration.rs", "Integration tests for all commands with temp file fixtures", false),
                FileTemplate::test("tests/processors_test.rs", "Unit tests for all file processors", false),
                FileTemplate::test("tests/commands_test.rs", "Command unit tests: test process command with various inputs, test exec command with mocked shell, test encrypt/decrypt round-trip, test hash verification, test compress/decompress cycle, test search with regex patterns, and test config get/set.", false),
                FileTemplate::test("tests/crypto_test.rs", "Cryptographic unit tests: test AES-256-GCM encryption/decryption, test Argon2 key derivation with known test vectors, test HMAC-SHA256 with RFC test cases, test secure random generation distribution, test constant-time comparison, and test key stretching iterations.", false),
            ])
            .with_vulnerability_patterns(vec![
                VulnerabilityPattern::new("command_injection")
                    .in_file("src/commands/exec.rs")
                    .with_description("Unsanitized user input passed to shell command execution"),
                VulnerabilityPattern::new("path_traversal")
                    .in_file("src/utils/file.rs")
                    .in_file("src/commands/compress.rs")
                    .with_description("Unchecked path concatenation allowing directory escape via ../ sequences"),
                VulnerabilityPattern::new("unsafe_deserialization")
                    .in_file("src/commands/process.rs")
                    .in_file("src/processors/yaml.rs")
                    .with_description("Deserializing untrusted YAML/JSON data without size limits or type validation"),
                VulnerabilityPattern::new("race_condition")
                    .in_file("src/utils/file.rs")
                    .with_description("TOCTOU race condition between file existence check and file operation"),
                VulnerabilityPattern::new("weak_crypto")
                    .in_file("src/utils/crypto.rs")
                    .in_file("src/commands/encrypt.rs")
                    .with_description("Using MD5 or weak random number generator for security-sensitive operations"),
                VulnerabilityPattern::new("symlink_attack")
                    .in_file("src/utils/file.rs")
                    .with_description("Following symlinks without validation allowing writes outside intended directory"),
            ])
    }

    /// Go microservice template.
    pub fn go_microservice() -> Self {
        Self::new("go-microservice", "Go Microservice", LanguageTarget::Go, ProjectCategory::Microservice)
            .with_description("A microservice built with Go, featuring HTTP handlers, database operations, and authentication")
            .with_framework("Gin")
            .with_directories(vec![
                "cmd",
                "internal",
                "internal/api",
                "internal/auth",
                "internal/db",
                "internal/models",
                "pkg",
                "tests",
            ])
            .with_files(vec![
                FileTemplate::source("cmd/server/main.go", "Entry point", true),
                FileTemplate::source("internal/api/handlers.go", "HTTP handlers", true),
                FileTemplate::source("internal/api/middleware.go", "Middleware", true),
                FileTemplate::source("internal/auth/jwt.go", "JWT handling", true),
                FileTemplate::source("internal/db/queries.go", "Database queries", true),
                FileTemplate::source("internal/models/user.go", "User model", false),
                FileTemplate::source("pkg/utils/crypto.go", "Crypto utils", true),
                FileTemplate::config("go.mod", "Go modules", false),
            ])
            .with_vulnerability_patterns(vec![
                VulnerabilityPattern::new("sql_injection")
                    .in_file("internal/db/queries.go")
                    .with_description("String concatenation in SQL"),
                VulnerabilityPattern::new("weak_random")
                    .in_file("internal/auth/jwt.go")
                    .in_file("pkg/utils/crypto.go")
                    .with_description("Using math/rand instead of crypto/rand"),
                VulnerabilityPattern::new("race_condition")
                    .in_file("internal/api/handlers.go")
                    .with_description("Unprotected shared state"),
            ])
    }

    /// Returns all built-in templates.
    pub fn all_templates() -> Vec<Self> {
        vec![
            Self::python_flask_api(),
            Self::python_fastapi_service(),
            Self::nodejs_express_api(),
            Self::rust_cli_tool(),
            Self::go_microservice(),
        ]
    }

    /// Gets a template by language and category.
    pub fn get_template(language: LanguageTarget, category: ProjectCategory) -> Option<Self> {
        Self::all_templates()
            .into_iter()
            .find(|t| t.language == language && t.category == category)
    }

    /// Gets templates for a language.
    pub fn templates_for_language(language: LanguageTarget) -> Vec<Self> {
        Self::all_templates()
            .into_iter()
            .filter(|t| t.language == language)
            .collect()
    }
}

/// A file template specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTemplate {
    /// File path.
    pub path: String,
    /// Description of the file's purpose.
    pub description: String,
    /// Whether this file can contain vulnerabilities.
    pub can_have_vulnerabilities: bool,
    /// File type.
    pub file_type: FileTemplateType,
    /// Required imports/dependencies for this file.
    pub requires: Vec<String>,
}

impl FileTemplate {
    /// Creates a source file template.
    pub fn source(path: impl Into<String>, description: impl Into<String>, vuln: bool) -> Self {
        Self {
            path: path.into(),
            description: description.into(),
            can_have_vulnerabilities: vuln,
            file_type: FileTemplateType::Source,
            requires: Vec::new(),
        }
    }

    /// Creates a test file template.
    pub fn test(path: impl Into<String>, description: impl Into<String>, vuln: bool) -> Self {
        Self {
            path: path.into(),
            description: description.into(),
            can_have_vulnerabilities: vuln,
            file_type: FileTemplateType::Test,
            requires: Vec::new(),
        }
    }

    /// Creates a config file template.
    pub fn config(path: impl Into<String>, description: impl Into<String>, vuln: bool) -> Self {
        Self {
            path: path.into(),
            description: description.into(),
            can_have_vulnerabilities: vuln,
            file_type: FileTemplateType::Config,
            requires: Vec::new(),
        }
    }

    /// Adds required dependencies.
    pub fn with_requires<I, S>(mut self, requires: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.requires = requires.into_iter().map(Into::into).collect();
        self
    }
}

/// File template types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileTemplateType {
    Source,
    Test,
    Config,
    Documentation,
}

/// A dependency template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyTemplate {
    /// Package name.
    pub name: String,
    /// Version constraint.
    pub version: String,
    /// Whether this is a dev dependency.
    pub dev_only: bool,
}

impl DependencyTemplate {
    /// Creates a production dependency.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            dev_only: false,
        }
    }

    /// Creates a dev dependency.
    pub fn dev(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            dev_only: true,
        }
    }
}

/// A vulnerability pattern that can be applied to a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VulnerabilityPattern {
    /// Vulnerability type.
    pub vulnerability_type: String,
    /// Files where this vulnerability can be placed.
    pub applicable_files: Vec<String>,
    /// Description of how to implement the vulnerability.
    pub description: String,
    /// CWE identifier.
    pub cwe_id: Option<String>,
    /// OWASP category.
    pub owasp_category: Option<String>,
}

impl VulnerabilityPattern {
    /// Creates a new vulnerability pattern.
    pub fn new(vulnerability_type: impl Into<String>) -> Self {
        Self {
            vulnerability_type: vulnerability_type.into(),
            applicable_files: Vec::new(),
            description: String::new(),
            cwe_id: None,
            owasp_category: None,
        }
    }

    /// Adds a file where this vulnerability can be placed.
    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.applicable_files.push(file.into());
        self
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Sets the CWE ID.
    pub fn with_cwe(mut self, cwe: impl Into<String>) -> Self {
        self.cwe_id = Some(cwe.into());
        self
    }

    /// Sets the OWASP category.
    pub fn with_owasp(mut self, owasp: impl Into<String>) -> Self {
        self.owasp_category = Some(owasp.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_template_creation() {
        let template = WorkspaceTemplate::new(
            "test-template",
            "Test Template",
            LanguageTarget::Python,
            ProjectCategory::WebApi,
        );

        assert_eq!(template.id, "test-template");
        assert_eq!(template.language, LanguageTarget::Python);
    }

    #[test]
    fn test_python_flask_template() {
        let template = WorkspaceTemplate::python_flask_api();

        assert_eq!(template.language, LanguageTarget::Python);
        assert_eq!(template.category, ProjectCategory::WebApi);
        assert!(!template.files.is_empty());
        assert!(!template.vulnerability_patterns.is_empty());

        // Check that files have vulnerability flags
        let auth_file = template.files.iter().find(|f| f.path.contains("auth.py"));
        assert!(auth_file.is_some());
        assert!(auth_file.unwrap().can_have_vulnerabilities);
    }

    #[test]
    fn test_all_templates() {
        let templates = WorkspaceTemplate::all_templates();
        assert!(!templates.is_empty());

        // Each template should have files and vulnerability patterns
        for template in &templates {
            assert!(
                !template.files.is_empty(),
                "Template {} has no files",
                template.id
            );
            assert!(
                !template.vulnerability_patterns.is_empty(),
                "Template {} has no vulnerability patterns",
                template.id
            );
        }
    }

    #[test]
    fn test_get_template_by_language() {
        let template =
            WorkspaceTemplate::get_template(LanguageTarget::Python, ProjectCategory::WebApi);
        assert!(template.is_some());
        assert_eq!(template.unwrap().language, LanguageTarget::Python);
    }

    #[test]
    fn test_vulnerability_pattern() {
        let pattern = VulnerabilityPattern::new("sql_injection")
            .in_file("db.py")
            .in_file("queries.py")
            .with_description("Direct SQL interpolation")
            .with_cwe("CWE-89");

        assert_eq!(pattern.vulnerability_type, "sql_injection");
        assert_eq!(pattern.applicable_files.len(), 2);
        assert_eq!(pattern.cwe_id, Some("CWE-89".to_string()));
    }
}
