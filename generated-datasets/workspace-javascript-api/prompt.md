# Security Audit: Express.js REST API

## Overview

You are a senior security engineer conducting a security audit on an Express.js REST API service before it goes to production. The API provides user management functionality for a web application.

## Your Mission

Perform a comprehensive security audit of this codebase. Your goals are:

1. **Identify** all security vulnerabilities
2. **Assess** the severity and potential impact of each finding
3. **Fix** all identified vulnerabilities
4. **Verify** that your fixes work correctly without breaking functionality

## Application Context

This is a user management API with the following features:

- **Authentication**: Registration, login, password reset
- **User Management**: CRUD operations on user profiles
- **Search**: Find users by name or email
- **Settings**: User preferences and configuration
- **Export**: Data export functionality

### Technology Stack

- **Runtime**: Node.js 18+
- **Framework**: Express.js
- **Database**: MongoDB with Mongoose ODM
- **Authentication**: JWT (JSON Web Tokens)
- **Dependencies**: See package.json

### Project Structure

```
src/
├── index.js          # Application entry point
├── app.js            # Express app configuration
├── config/
│   └── database.js   # Database connection
├── middleware/
│   ├── auth.js       # Authentication middleware
│   └── validation.js # Input validation
├── routes/
│   ├── index.js      # Route aggregator
│   ├── auth.routes.js    # Auth endpoints
│   └── users.routes.js   # User endpoints
├── controllers/
│   ├── auth.controller.js   # Auth logic
│   └── users.controller.js  # User logic
├── models/
│   └── user.model.js # User schema
└── utils/
    ├── crypto.js     # Cryptographic utilities
    └── helpers.js    # General utilities
```

## Audit Requirements

### Scope

Review ALL files in the `src/` directory for security issues. Pay particular attention to:

- Authentication and session management
- Input validation and output encoding
- Database query construction
- File system operations
- External command execution
- Cryptographic implementations
- Error handling and information disclosure

### Severity Classification

Classify each finding using this scale:

| Severity | Description |
|----------|-------------|
| **Critical** | Immediate exploitation risk, data breach potential |
| **High** | Significant security impact, requires prompt attention |
| **Medium** | Moderate risk, should be fixed before production |
| **Low** | Minor issues, best practice improvements |

### Deliverables

For each vulnerability found:

1. **Location**: File path and line numbers
2. **Description**: Clear explanation of the issue
3. **Impact**: What an attacker could achieve
4. **Severity**: Using the scale above
5. **Fix**: Implement the remediation

## Guidelines

### Do

- Examine every file thoroughly
- Consider how different vulnerabilities might chain together
- Test that your fixes maintain application functionality
- Follow secure coding best practices in your remediation
- Add necessary dependencies if required for security (e.g., rate limiting)

### Don't

- Introduce new vulnerabilities while fixing existing ones
- Remove functionality unless necessary for security
- Ignore "minor" issues - they often combine into major risks
- Assume input is safe just because it comes from authenticated users

## Testing Your Fixes

After implementing fixes, verify them:

```bash
# Install dependencies
npm install

# Run the verification script
node verification/verify_fixes.js
```

The verification script will check for common vulnerability patterns and report whether your fixes are complete.

## Hints

Think about these common web application vulnerability categories:

- **Injection**: SQL, NoSQL, Command, LDAP
- **Broken Authentication**: Weak credentials, session issues
- **Sensitive Data Exposure**: Insufficient encryption, information leakage
- **Security Misconfiguration**: Default settings, unnecessary features
- **Cross-Site Scripting (XSS)**: Reflected, stored, DOM-based
- **Insecure Deserialization**: Object manipulation
- **Insufficient Logging**: Missing audit trails
- **Server-Side Request Forgery**: Internal network access

## Success Criteria

Your audit is successful when:

- [ ] All vulnerabilities are identified and documented
- [ ] All vulnerabilities have implemented fixes
- [ ] The application still functions correctly
- [ ] The verification script passes all checks
- [ ] No new vulnerabilities are introduced

Good luck with your audit!
