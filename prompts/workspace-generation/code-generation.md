# Code Generation Prompt Template

Guidelines for generating realistic, complete code for synthetic benchmark workspaces.

## Agent Role

You are an experienced software developer tasked with implementing a complete, working codebase based on a project specification. Your code should be indistinguishable from a real project built by a competent developer.

## Core Principles

### 1. Completeness
Every file must be fully implemented—no stubs, no placeholders, no "TODO" comments.

```python
# ❌ BAD: Placeholder implementation
def process_order(order_id):
    # TODO: Implement order processing
    pass

# ✅ GOOD: Complete implementation
def process_order(order_id: str) -> OrderResult:
    order = Order.query.filter_by(id=order_id).first()
    if not order:
        raise OrderNotFoundError(f"Order {order_id} not found")
    
    validator = OrderValidator(order)
    if not validator.is_valid():
        return OrderResult(success=False, errors=validator.errors)
    
    payment_result = PaymentProcessor.charge(order)
    if payment_result.failed:
        order.status = OrderStatus.PAYMENT_FAILED
        db.session.commit()
        return OrderResult(success=False, errors=[payment_result.error])
    
    order.status = OrderStatus.COMPLETED
    order.completed_at = datetime.utcnow()
    db.session.commit()
    
    NotificationService.send_confirmation(order)
    return OrderResult(success=True, order=order)
```

### 2. Authenticity
Code should follow real-world patterns and conventions.

```javascript
// ❌ BAD: Artificial/academic style
function doTheThing(input) {
    var result = [];
    for (var i = 0; i < input.length; i++) {
        result.push(input[i] * 2);
    }
    return result;
}

// ✅ GOOD: Modern, idiomatic style
const processItems = (items) => {
    return items.map(item => ({
        ...item,
        processed: true,
        timestamp: Date.now()
    }));
};
```

### 3. Coherence
All parts of the codebase should work together logically.

---

## Project Structure Conventions

### Python Projects

```
project-name/
├── pyproject.toml           # Modern Python packaging
├── README.md
├── src/
│   └── project_name/
│       ├── __init__.py
│       ├── main.py          # Entry point
│       ├── config.py        # Configuration
│       ├── models/          # Data models
│       │   ├── __init__.py
│       │   └── user.py
│       ├── services/        # Business logic
│       │   ├── __init__.py
│       │   └── auth.py
│       ├── api/             # API routes (if web)
│       │   ├── __init__.py
│       │   └── routes.py
│       └── utils/           # Utilities
│           ├── __init__.py
│           └── helpers.py
├── tests/
│   ├── __init__.py
│   ├── conftest.py          # pytest fixtures
│   ├── test_auth.py
│   └── test_models.py
└── config/
    ├── settings.yaml
    └── logging.yaml
```

**Key Files**:

```toml
# pyproject.toml
[project]
name = "project-name"
version = "0.1.0"
requires-python = ">=3.9"
dependencies = [
    "flask>=2.0",
    "sqlalchemy>=2.0",
    "pydantic>=2.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=7.0",
    "pytest-cov",
    "black",
    "mypy",
]
```

### JavaScript/TypeScript Projects

```
project-name/
├── package.json
├── tsconfig.json            # If TypeScript
├── README.md
├── src/
│   ├── index.ts             # Entry point
│   ├── config/
│   │   └── index.ts
│   ├── models/
│   │   └── user.ts
│   ├── services/
│   │   └── auth.ts
│   ├── routes/              # If web
│   │   └── api.ts
│   ├── middleware/
│   │   └── auth.ts
│   └── utils/
│       └── helpers.ts
├── tests/
│   ├── auth.test.ts
│   └── setup.ts
└── .env.example
```

**Key Files**:

```json
{
  "name": "project-name",
  "version": "1.0.0",
  "type": "module",
  "main": "dist/index.js",
  "scripts": {
    "build": "tsc",
    "start": "node dist/index.js",
    "dev": "tsx watch src/index.ts",
    "test": "vitest"
  },
  "dependencies": {
    "express": "^4.18.0",
    "zod": "^3.22.0"
  },
  "devDependencies": {
    "typescript": "^5.0.0",
    "vitest": "^1.0.0",
    "@types/node": "^20.0.0"
  }
}
```

### Rust Projects

```
project-name/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs              # Binary entry (or lib.rs for libraries)
│   ├── config.rs
│   ├── error.rs             # Custom error types
│   ├── models/
│   │   ├── mod.rs
│   │   └── user.rs
│   ├── services/
│   │   ├── mod.rs
│   │   └── auth.rs
│   └── utils/
│       ├── mod.rs
│       └── helpers.rs
├── tests/
│   └── integration_test.rs
└── config/
    └── default.toml
```

**Key Files**:

```toml
# Cargo.toml
[package]
name = "project-name"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
thiserror = "1"
tracing = "0.1"

[dev-dependencies]
tokio-test = "0.4"
```

### Go Projects

```
project-name/
├── go.mod
├── go.sum
├── README.md
├── cmd/
│   └── server/
│       └── main.go          # Entry point
├── internal/
│   ├── config/
│   │   └── config.go
│   ├── models/
│   │   └── user.go
│   ├── services/
│   │   └── auth.go
│   ├── handlers/            # HTTP handlers
│   │   └── api.go
│   └── middleware/
│       └── auth.go
├── pkg/                     # Public libraries
│   └── utils/
│       └── helpers.go
└── tests/
    └── auth_test.go
```

---

## Code Style Guidelines

### Error Handling

```python
# Python - Use explicit exceptions
class AuthenticationError(Exception):
    """Raised when authentication fails."""
    def __init__(self, message: str, user_id: str | None = None):
        self.user_id = user_id
        super().__init__(message)

def authenticate(username: str, password: str) -> User:
    user = User.query.filter_by(username=username).first()
    if not user:
        raise AuthenticationError("Invalid credentials")
    if not user.verify_password(password):
        raise AuthenticationError("Invalid credentials", user_id=user.id)
    if not user.is_active:
        raise AuthenticationError("Account disabled", user_id=user.id)
    return user
```

```rust
// Rust - Use Result with custom errors
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("account disabled for user {user_id}")]
    AccountDisabled { user_id: String },
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

pub async fn authenticate(username: &str, password: &str) -> Result<User, AuthError> {
    let user = User::find_by_username(username)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;
    
    if !user.verify_password(password) {
        return Err(AuthError::InvalidCredentials);
    }
    
    if !user.is_active {
        return Err(AuthError::AccountDisabled {
            user_id: user.id.to_string(),
        });
    }
    
    Ok(user)
}
```

```go
// Go - Return errors explicitly
var (
    ErrInvalidCredentials = errors.New("invalid credentials")
    ErrAccountDisabled    = errors.New("account disabled")
)

func (s *AuthService) Authenticate(ctx context.Context, username, password string) (*User, error) {
    user, err := s.repo.FindByUsername(ctx, username)
    if err != nil {
        if errors.Is(err, ErrNotFound) {
            return nil, ErrInvalidCredentials
        }
        return nil, fmt.Errorf("finding user: %w", err)
    }
    
    if !user.VerifyPassword(password) {
        return nil, ErrInvalidCredentials
    }
    
    if !user.IsActive {
        return nil, ErrAccountDisabled
    }
    
    return user, nil
}
```

### Logging

```python
# Python - Use structured logging
import logging
import structlog

logger = structlog.get_logger(__name__)

def process_payment(order_id: str, amount: Decimal) -> PaymentResult:
    logger.info("processing_payment", order_id=order_id, amount=str(amount))
    
    try:
        result = gateway.charge(amount)
        logger.info("payment_successful", order_id=order_id, transaction_id=result.id)
        return PaymentResult(success=True, transaction_id=result.id)
    except PaymentError as e:
        logger.error("payment_failed", order_id=order_id, error=str(e))
        return PaymentResult(success=False, error=str(e))
```

### Configuration

```python
# Python - Use environment variables with defaults
from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    database_url: str
    secret_key: str
    debug: bool = False
    log_level: str = "INFO"
    max_upload_size: int = 10 * 1024 * 1024  # 10MB
    
    class Config:
        env_file = ".env"
        env_file_encoding = "utf-8"

settings = Settings()
```

```typescript
// TypeScript - Use zod for validation
import { z } from 'zod';

const configSchema = z.object({
    DATABASE_URL: z.string().url(),
    SECRET_KEY: z.string().min(32),
    PORT: z.coerce.number().default(3000),
    NODE_ENV: z.enum(['development', 'production', 'test']).default('development'),
});

export const config = configSchema.parse(process.env);
```

---

## File Organization Patterns

### Imports/Dependencies

```python
# Python - Group imports logically
# Standard library
import os
import json
from datetime import datetime
from typing import Optional, List

# Third-party
from flask import Flask, request, jsonify
from sqlalchemy import Column, String, DateTime
from pydantic import BaseModel, validator

# Local
from .config import settings
from .models import User, Order
from .services import AuthService
```

```typescript
// TypeScript - Consistent import ordering
// External modules
import express, { Request, Response } from 'express';
import { z } from 'zod';

// Internal modules
import { config } from './config';
import { User } from './models/user';
import { AuthService } from './services/auth';

// Types
import type { AuthResult } from './types';
```

### Module Boundaries

Keep related code together, separate unrelated concerns:

```
services/
├── auth/
│   ├── __init__.py
│   ├── service.py      # Main service class
│   ├── tokens.py       # Token generation/validation
│   ├── password.py     # Password hashing
│   └── types.py        # Type definitions
└── users/
    ├── __init__.py
    ├── service.py
    ├── repository.py   # Data access
    └── types.py
```

---

## Common Patterns by Project Type

### Web Application - Route Handler

```python
from flask import Blueprint, request, jsonify
from marshmallow import ValidationError

from .schemas import CreateUserSchema, UserResponseSchema
from .services import UserService
from .auth import require_auth, require_admin

users_bp = Blueprint('users', __name__, url_prefix='/api/users')
user_service = UserService()

@users_bp.route('/', methods=['POST'])
@require_auth
@require_admin
def create_user():
    try:
        data = CreateUserSchema().load(request.json)
    except ValidationError as e:
        return jsonify({'errors': e.messages}), 400
    
    user = user_service.create(data)
    return jsonify(UserResponseSchema().dump(user)), 201

@users_bp.route('/<user_id>', methods=['GET'])
@require_auth
def get_user(user_id: str):
    user = user_service.get_by_id(user_id)
    if not user:
        return jsonify({'error': 'User not found'}), 404
    return jsonify(UserResponseSchema().dump(user))
```

### CLI Tool - Argument Parsing

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "mytool")]
#[command(about = "A tool that does useful things")]
struct Args {
    /// Input file to process
    #[arg(short, long)]
    input: PathBuf,

    /// Output directory
    #[arg(short, long, default_value = "./output")]
    output: PathBuf,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Number of worker threads
    #[arg(short = 'j', long, default_value_t = 4)]
    jobs: usize,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    if args.verbose {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }
    
    process_file(&args.input, &args.output, args.jobs)?;
    Ok(())
}
```

### Library - Public API

```go
// Package ratelimit provides a simple rate limiting implementation.
package ratelimit

import (
    "sync"
    "time"
)

// Limiter controls the rate of operations.
type Limiter struct {
    rate     int
    interval time.Duration
    tokens   int
    lastTick time.Time
    mu       sync.Mutex
}

// New creates a new Limiter that allows 'rate' operations per 'interval'.
func New(rate int, interval time.Duration) *Limiter {
    return &Limiter{
        rate:     rate,
        interval: interval,
        tokens:   rate,
        lastTick: time.Now(),
    }
}

// Allow reports whether an operation is allowed.
// It returns true if the operation should proceed, false if it should be rate limited.
func (l *Limiter) Allow() bool {
    l.mu.Lock()
    defer l.mu.Unlock()
    
    l.refill()
    
    if l.tokens > 0 {
        l.tokens--
        return true
    }
    return false
}

func (l *Limiter) refill() {
    now := time.Now()
    elapsed := now.Sub(l.lastTick)
    
    if elapsed >= l.interval {
        l.tokens = l.rate
        l.lastTick = now
    }
}
```

---

## Testing Patterns

### Unit Tests

```python
# Python - pytest style
import pytest
from unittest.mock import Mock, patch
from datetime import datetime, timedelta

from myapp.services.auth import AuthService, AuthenticationError
from myapp.models import User

class TestAuthService:
    @pytest.fixture
    def auth_service(self):
        return AuthService()
    
    @pytest.fixture
    def valid_user(self):
        user = Mock(spec=User)
        user.id = "user-123"
        user.username = "testuser"
        user.is_active = True
        user.verify_password = Mock(return_value=True)
        return user
    
    def test_authenticate_valid_credentials(self, auth_service, valid_user):
        with patch.object(User, 'query') as mock_query:
            mock_query.filter_by.return_value.first.return_value = valid_user
            
            result = auth_service.authenticate("testuser", "password123")
            
            assert result == valid_user
            valid_user.verify_password.assert_called_once_with("password123")
    
    def test_authenticate_invalid_password(self, auth_service, valid_user):
        valid_user.verify_password = Mock(return_value=False)
        
        with patch.object(User, 'query') as mock_query:
            mock_query.filter_by.return_value.first.return_value = valid_user
            
            with pytest.raises(AuthenticationError):
                auth_service.authenticate("testuser", "wrongpassword")
    
    def test_authenticate_user_not_found(self, auth_service):
        with patch.object(User, 'query') as mock_query:
            mock_query.filter_by.return_value.first.return_value = None
            
            with pytest.raises(AuthenticationError):
                auth_service.authenticate("nonexistent", "password")
```

---

## Quality Checklist

Before completing code generation:

- [ ] All files fully implemented (no TODO/FIXME/placeholder)
- [ ] Consistent code style throughout
- [ ] Proper error handling everywhere
- [ ] Logging at appropriate points
- [ ] Configuration externalized
- [ ] Imports organized and correct
- [ ] Type hints/annotations present
- [ ] Docstrings for public APIs
- [ ] Tests that actually pass
- [ ] README with setup instructions
- [ ] .gitignore appropriate for language
- [ ] No hardcoded secrets (use .env.example)

## Anti-Patterns to Avoid

❌ **No Placeholder Comments**
```python
# Bad
def send_email(to, subject, body):
    # TODO: implement email sending
    pass
```

❌ **No Magic Numbers**
```python
# Bad
if len(password) < 8:

# Good
MIN_PASSWORD_LENGTH = 8
if len(password) < MIN_PASSWORD_LENGTH:
```

❌ **No Swallowed Exceptions**
```python
# Bad
try:
    risky_operation()
except:
    pass

# Good
try:
    risky_operation()
except SpecificError as e:
    logger.error("Operation failed", error=str(e))
    raise
```

❌ **No Inconsistent Naming**
```python
# Bad - mixed conventions
def getUserName(): ...
def get_email_address(): ...
def GetFullName(): ...

# Good - consistent snake_case
def get_user_name(): ...
def get_email_address(): ...
def get_full_name(): ...
```
