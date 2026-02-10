#!/usr/bin/env node

/**
 * Security Fix Verification Script
 * 
 * This script verifies that security vulnerabilities have been properly fixed
 * in the Express.js API codebase.
 */

const fs = require('fs');
const path = require('path');

const SRC_DIR = path.join(__dirname, '../src');

const COLORS = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
};

function log(color, message) {
  console.log(`${COLORS[color]}${message}${COLORS.reset}`);
}

function readFile(filePath) {
  const fullPath = path.join(SRC_DIR, filePath);
  if (!fs.existsSync(fullPath)) {
    return null;
  }
  return fs.readFileSync(fullPath, 'utf-8');
}

function checkPattern(content, pattern, shouldExist = false) {
  const regex = new RegExp(pattern, 'gm');
  const found = regex.test(content);
  return shouldExist ? found : !found;
}

const checks = [
  {
    id: 'jwt-hardcoded-secret',
    name: 'Hardcoded JWT Secret',
    severity: 'CRITICAL',
    description: 'JWT secret should not be hardcoded',
    file: 'middleware/auth.js',
    verify: (content) => {
      const hasHardcoded = /JWT_SECRET\s*=\s*['"][^'"]+['"]/g.test(content);
      const usesEnv = /process\.env\.JWT_SECRET/g.test(content);
      return !hasHardcoded && usesEnv;
    },
    hint: 'Use process.env.JWT_SECRET instead of hardcoded value',
  },
  {
    id: 'jwt-no-expiration',
    name: 'JWT Token Expiration',
    severity: 'CRITICAL',
    description: 'JWT tokens should have expiration time',
    file: 'middleware/auth.js',
    verify: (content) => {
      return /jwt\.sign\([^)]+,\s*\{[^}]*expiresIn/g.test(content);
    },
    hint: 'Add { expiresIn: "24h" } or similar to jwt.sign() options',
  },
  {
    id: 'nosql-injection-where',
    name: 'NoSQL Injection ($where)',
    severity: 'CRITICAL',
    description: '$where operator should not be used with user input',
    file: 'controllers/users.controller.js',
    verify: (content) => {
      return !(/\$where/g.test(content));
    },
    hint: 'Replace $where with $regex or $text search',
  },
  {
    id: 'xss-html-injection',
    name: 'XSS in Profile Response',
    severity: 'HIGH',
    description: 'User data should be escaped before HTML rendering',
    file: 'controllers/users.controller.js',
    verify: (content) => {
      const hasUnsafeHtml = /`[\s\S]*<[^>]+>[\s\S]*\$\{user\.(name|bio)\}[\s\S]*`/g.test(content);
      if (!hasUnsafeHtml) return true;
      
      const hasEscaping = /escapeHtml|sanitize|encode|DOMPurify/gi.test(content);
      return hasEscaping;
    },
    hint: 'Escape HTML entities or return JSON instead of HTML',
  },
  {
    id: 'path-traversal',
    name: 'Path Traversal in File Serving',
    severity: 'HIGH',
    description: 'Filename should be sanitized to prevent directory traversal',
    file: 'app.js',
    verify: (content) => {
      const hasFileServing = /\/files\/:filename/g.test(content);
      if (!hasFileServing) return true;
      
      const hasSanitization = /path\.basename|\.replace\([^)]*\.\./g.test(content);
      const hasValidation = /\.includes\(['"]\.\.['"]|indexOf\(['"]\.\.['"]/.test(content);
      return hasSanitization || hasValidation;
    },
    hint: 'Use path.basename() or validate filename does not contain ".."',
  },
  {
    id: 'command-injection-export',
    name: 'Command Injection in Export',
    severity: 'CRITICAL',
    description: 'User data should not be passed to shell commands',
    file: 'controllers/users.controller.js',
    verify: (content) => {
      const hasUnsafeExec = /exec\s*\(\s*command\s*,/g.test(content);
      const hasUserDataInCommand = /const command.*\$\{.*userData|echo.*\$\{.*userData|echo '\$\{userData\}/g.test(content);
      
      if (hasUnsafeExec && hasUserDataInCommand) {
        return false;
      }
      
      const hasExecWithUserInput = /exec\s*\([^)]*\$\{[^}]*(user|id|filename)/gi.test(content);
      return !hasExecWithUserInput;
    },
    hint: 'Use fs.writeFile() instead of shell commands',
  },
  {
    id: 'command-injection-helper',
    name: 'Command Injection in Helper',
    severity: 'HIGH',
    description: 'User input should not be concatenated into shell commands',
    file: 'utils/helpers.js',
    verify: (content) => {
      const hasCommandWithInterpolation = /const\s+command\s*=\s*`[^`]*\$\{/g.test(content);
      const hasExec = /exec\s*\(\s*command/g.test(content);
      
      if (hasCommandWithInterpolation && hasExec) {
        const hasWhitelist = /allowedFiles|allowedTypes|whitelist/gi.test(content);
        const usesFs = /fs\.readFile|readFileSync/g.test(content);
        return hasWhitelist || usesFs;
      }
      return true;
    },
    hint: 'Use fs.readFile() or validate input against whitelist',
  },
  {
    id: 'prototype-pollution',
    name: 'Prototype Pollution in mergeDeep',
    severity: 'HIGH',
    description: 'Object merging should filter dangerous keys early',
    file: 'utils/helpers.js',
    verify: (content) => {
      const hasMerge = /function\s+mergeDeep/g.test(content);
      if (!hasMerge) return true;
      
      const forEachMatch = content.match(/Object\.keys\(source\)\.forEach\s*\(\s*\(?key\)?\s*=>\s*\{[\s\S]*?\}\s*\)/g);
      if (forEachMatch) {
        const forEachCode = forEachMatch[0];
        const hasEarlyCheck = /if\s*\([^)]*(__proto__|constructor|prototype)[^)]*\)[\s\S]*?(continue|return)/g.test(forEachCode);
        if (!hasEarlyCheck) {
          return false;
        }
      }
      
      const forInLoop = content.match(/for\s*\(\s*(const|let|var)\s+\w+\s+in\s+source\s*\)[\s\S]*?}/g);
      if (forInLoop) {
        const forInCode = forInLoop[0];
        const hasEarlyCheck = /if\s*\([^)]*(__proto__|constructor|prototype)[^)]*\)[\s\S]*?continue/g.test(forInCode);
        if (!hasEarlyCheck) {
          return false;
        }
      }
      
      return true;
    },
    hint: 'Check for __proto__, constructor, prototype before Object.assign in ALL loops',
  },
  {
    id: 'rate-limiting',
    name: 'Rate Limiting on Auth Endpoints',
    severity: 'MEDIUM',
    description: 'Authentication endpoints should have rate limiting',
    file: 'routes/auth.routes.js',
    verify: (content) => {
      const hasRateLimit = /rateLimit|rateLimiter|limiter/gi.test(content);
      const hasMiddleware = /router\.(post|get)\s*\(\s*['"]\/login['"],\s*\w+Limit/gi.test(content);
      return hasRateLimit || hasMiddleware;
    },
    hint: 'Add express-rate-limit middleware to login and register routes',
  },
  {
    id: 'rate-limit-imported',
    name: 'Rate Limiter Dependency',
    severity: 'MEDIUM',
    description: 'Rate limiting package should be imported',
    file: 'routes/auth.routes.js',
    verify: (content) => {
      const hasImport = /require\(['"]express-rate-limit['"]\)/g.test(content);
      const hasRateLimitUsage = /rateLimit\s*\(/g.test(content);
      return hasImport || !hasRateLimitUsage;
    },
    hint: 'Import express-rate-limit: const rateLimit = require("express-rate-limit")',
  },
];

function runChecks() {
  log('cyan', '\n╔════════════════════════════════════════════════════════════╗');
  log('cyan', '║         Security Fix Verification Report                    ║');
  log('cyan', '╚════════════════════════════════════════════════════════════╝\n');

  let passed = 0;
  let failed = 0;
  const results = [];

  for (const check of checks) {
    const filePath = check.file.startsWith('../') 
      ? check.file.substring(3) 
      : check.file;
    const content = readFile(check.file);
    
    if (content === null) {
      log('yellow', `⚠ SKIP: ${check.name}`);
      log('yellow', `  File not found: ${filePath}\n`);
      continue;
    }

    const success = check.verify(content);
    
    if (success) {
      passed++;
      log('green', `✓ PASS: ${check.name}`);
      log('blue', `  [${check.severity}] ${check.description}\n`);
    } else {
      failed++;
      log('red', `✗ FAIL: ${check.name}`);
      log('blue', `  [${check.severity}] ${check.description}`);
      log('yellow', `  Hint: ${check.hint}\n`);
    }

    results.push({
      id: check.id,
      name: check.name,
      severity: check.severity,
      passed: success,
    });
  }

  log('cyan', '════════════════════════════════════════════════════════════');
  log('cyan', '                      Summary');
  log('cyan', '════════════════════════════════════════════════════════════\n');

  const total = passed + failed;
  const percentage = total > 0 ? Math.round((passed / total) * 100) : 0;

  log('blue', `  Total Checks:  ${total}`);
  log('green', `  Passed:        ${passed}`);
  log('red', `  Failed:        ${failed}`);
  log('cyan', `  Score:         ${percentage}%\n`);

  const criticalFailed = results.filter(r => !r.passed && r.severity === 'CRITICAL').length;
  const highFailed = results.filter(r => !r.passed && r.severity === 'HIGH').length;

  if (criticalFailed > 0) {
    log('red', `  ⚠ ${criticalFailed} CRITICAL vulnerabilities remain unfixed!`);
  }
  if (highFailed > 0) {
    log('yellow', `  ⚠ ${highFailed} HIGH severity vulnerabilities remain unfixed!`);
  }

  if (failed === 0) {
    log('green', '\n  ✓ All security checks passed! Great work!\n');
    process.exit(0);
  } else {
    log('red', `\n  ✗ ${failed} security check(s) failed. Please review and fix.\n`);
    process.exit(1);
  }
}

if (require.main === module) {
  runChecks();
}

module.exports = { runChecks, checks };
