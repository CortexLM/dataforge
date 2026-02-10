# Security Audit: Flask REST API

## Background

You have been assigned to perform a security audit of a Flask REST API before it goes into production. The API handles user authentication, session management, and user data operations for a web application.

The development team has implemented the core functionality but needs an independent security review to ensure the application meets security standards before deployment.

## Your Task

Conduct a thorough security audit of this Flask REST API and remediate any vulnerabilities you discover.

### Scope

The entire codebase is in scope for this audit:

- `app/` - Main application code
  - `routes/` - API endpoint handlers
  - `models/` - Data models
  - `utils/` - Utility functions
  - `database.py` - Database connection handling
- `config.py` - Application configuration

### Requirements

1. **Identify Security Issues**
   - Review all source files for security vulnerabilities
   - Document each issue found with its location and severity
   - Consider OWASP Top 10 and common Python/Flask security pitfalls

2. **Fix All Vulnerabilities**
   - Implement secure coding practices
   - Ensure fixes don't break existing functionality
   - Follow the principle of least privilege

3. **Areas to Review**
   - Authentication and authorization mechanisms
   - Input validation and output encoding
   - Cryptographic implementations
   - Database query construction
   - Session management
   - File operations
   - Configuration and secrets management

### Constraints

- Do NOT add new features or endpoints
- Do NOT change the API contract (request/response formats)
- Do NOT modify the database schema
- Focus exclusively on security improvements

### Deliverables

After completing your audit and fixes:

1. List all vulnerabilities found
2. Explain the risk of each vulnerability
3. Describe the fix implemented for each issue
4. Verify that the application still functions correctly

## Technical Context

- **Framework**: Flask 3.0
- **Database**: PostgreSQL (via psycopg2)
- **Authentication**: JWT-based
- **Python Version**: 3.11+

## Getting Started

1. Review the project structure
2. Understand the authentication flow in `routes/auth.py`
3. Examine user operations in `routes/users.py`
4. Check utility functions in `utils/`
5. Review configuration in `config.py`

Good luck with your audit!
