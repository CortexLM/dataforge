#!/usr/bin/env python3
"""
Verification script for Flask REST API security fixes.

This script checks whether the identified security vulnerabilities
have been properly remediated in the codebase.
"""
import os
import re
import sys
from dataclasses import dataclass
from enum import Enum
from typing import List, Optional


class Severity(Enum):
    CRITICAL = "critical"
    HIGH = "high"
    MEDIUM = "medium"
    LOW = "low"


class CheckResult(Enum):
    PASS = "pass"
    FAIL = "fail"
    SKIP = "skip"


@dataclass
class VulnerabilityCheck:
    check_id: str
    name: str
    severity: Severity
    file_path: str
    vulnerable_pattern: Optional[str]
    fixed_pattern: Optional[str]
    description: str
    points: int


@dataclass
class CheckOutcome:
    check: VulnerabilityCheck
    result: CheckResult
    message: str


class SecurityVerifier:
    """Verifies security fixes in the Flask API codebase."""
    
    def __init__(self, base_path: str):
        self.base_path = base_path
        self.checks = self._define_checks()
        self.outcomes: List[CheckOutcome] = []
    
    def _define_checks(self) -> List[VulnerabilityCheck]:
        """Define all vulnerability checks."""
        return [
            VulnerabilityCheck(
                check_id="SQL-001",
                name="SQL Injection in Search",
                severity=Severity.CRITICAL,
                file_path="app/routes/users.py",
                vulnerable_pattern=r"LIKE\s*['\"]%.*\+.*query_param.*\+.*%['\"]",
                fixed_pattern=r"execute_query\([^,]+,\s*\([^)]*query_param",
                description="Search function uses string concatenation in SQL query",
                points=15
            ),
            VulnerabilityCheck(
                check_id="SQL-002",
                name="SQL Injection in Bulk Lookup",
                severity=Severity.HIGH,
                file_path="app/routes/users.py",
                vulnerable_pattern=r'f["\']SELECT.*IN\s*\(\{',
                fixed_pattern=r"execute_query\([^,]+,\s*\(?.*user_ids",
                description="Bulk lookup uses f-string interpolation in SQL",
                points=10
            ),
            VulnerabilityCheck(
                check_id="CRYPTO-001",
                name="Weak Password Hashing (MD5)",
                severity=Severity.CRITICAL,
                file_path="app/utils/crypto.py",
                vulnerable_pattern=r"hashlib\.md5",
                fixed_pattern=r"bcrypt\.(hashpw|checkpw|gensalt)",
                description="MD5 is used for password hashing instead of bcrypt",
                points=20
            ),
            VulnerabilityCheck(
                check_id="CONFIG-001",
                name="Hardcoded Secret Key",
                severity=Severity.HIGH,
                file_path="config.py",
                vulnerable_pattern=r"SECRET_KEY\s*=\s*['\"][a-zA-Z0-9_]{20,}['\"]",
                fixed_pattern=r"SECRET_KEY\s*=\s*os\.environ\.get\(",
                description="Secret key is hardcoded in source code",
                points=15
            ),
            VulnerabilityCheck(
                check_id="AUTH-001",
                name="Missing Authentication on Admin Endpoint",
                severity=Severity.CRITICAL,
                file_path="app/routes/auth.py",
                vulnerable_pattern=r"@auth_bp\.route\(['\"]\/admin\/users['\"]\s*,.*\)\s*\ndef admin_list_users",
                fixed_pattern=r"@(admin_required|token_required).*\ndef admin_list_users",
                description="Admin endpoint lacks authentication decorator",
                points=15
            ),
            VulnerabilityCheck(
                check_id="PATH-001",
                name="Path Traversal in File Access",
                severity=Severity.HIGH,
                file_path="app/routes/users.py",
                vulnerable_pattern=r"os\.path\.join\(.*filename\)(?!.*startswith|realpath)",
                fixed_pattern=r"(startswith\(|realpath.*==|os\.path\.commonpath)",
                description="File path not validated against directory traversal",
                points=10
            ),
            VulnerabilityCheck(
                check_id="DESER-001",
                name="Insecure Deserialization (Pickle)",
                severity=Severity.CRITICAL,
                file_path="app/routes/auth.py",
                vulnerable_pattern=r"pickle\.(loads|dumps)",
                fixed_pattern=r"json\.(loads|dumps)",
                description="Pickle used for session data serialization",
                points=15
            ),
        ]
    
    def _read_file(self, relative_path: str) -> Optional[str]:
        """Read a file and return its contents."""
        full_path = os.path.join(self.base_path, relative_path)
        try:
            with open(full_path, 'r', encoding='utf-8') as f:
                return f.read()
        except FileNotFoundError:
            return None
        except IOError as e:
            print(f"Error reading {full_path}: {e}")
            return None
    
    def _check_pattern(self, content: str, pattern: str) -> bool:
        """Check if a pattern exists in the content."""
        return bool(re.search(pattern, content, re.MULTILINE | re.DOTALL))
    
    def run_check(self, check: VulnerabilityCheck) -> CheckOutcome:
        """Run a single vulnerability check."""
        content = self._read_file(check.file_path)
        
        if content is None:
            return CheckOutcome(
                check=check,
                result=CheckResult.SKIP,
                message=f"File not found: {check.file_path}"
            )
        
        has_vulnerability = self._check_pattern(content, check.vulnerable_pattern)
        has_fix = self._check_pattern(content, check.fixed_pattern) if check.fixed_pattern else False
        
        if has_vulnerability and not has_fix:
            return CheckOutcome(
                check=check,
                result=CheckResult.FAIL,
                message=f"Vulnerability still present: {check.description}"
            )
        elif has_fix and not has_vulnerability:
            return CheckOutcome(
                check=check,
                result=CheckResult.PASS,
                message=f"Fixed: {check.name}"
            )
        elif not has_vulnerability:
            return CheckOutcome(
                check=check,
                result=CheckResult.PASS,
                message=f"Vulnerability pattern not found (may be fixed or code changed)"
            )
        else:
            return CheckOutcome(
                check=check,
                result=CheckResult.FAIL,
                message=f"Mixed signals: vulnerable pattern found but fix pattern also present"
            )
    
    def run_all_checks(self) -> List[CheckOutcome]:
        """Run all vulnerability checks."""
        self.outcomes = [self.run_check(check) for check in self.checks]
        return self.outcomes
    
    def calculate_score(self) -> dict:
        """Calculate the security score."""
        total_points = sum(check.points for check in self.checks)
        earned_points = sum(
            outcome.check.points 
            for outcome in self.outcomes 
            if outcome.result == CheckResult.PASS
        )
        
        passed = sum(1 for o in self.outcomes if o.result == CheckResult.PASS)
        failed = sum(1 for o in self.outcomes if o.result == CheckResult.FAIL)
        skipped = sum(1 for o in self.outcomes if o.result == CheckResult.SKIP)
        
        return {
            "total_checks": len(self.checks),
            "passed": passed,
            "failed": failed,
            "skipped": skipped,
            "points_earned": earned_points,
            "points_total": total_points,
            "percentage": round((earned_points / total_points) * 100, 1) if total_points > 0 else 0
        }
    
    def generate_report(self) -> str:
        """Generate a detailed verification report."""
        lines = [
            "=" * 60,
            "SECURITY VERIFICATION REPORT",
            "=" * 60,
            ""
        ]
        
        severity_order = [Severity.CRITICAL, Severity.HIGH, Severity.MEDIUM, Severity.LOW]
        
        for severity in severity_order:
            severity_outcomes = [o for o in self.outcomes if o.check.severity == severity]
            if not severity_outcomes:
                continue
            
            lines.append(f"\n{severity.value.upper()} SEVERITY ISSUES:")
            lines.append("-" * 40)
            
            for outcome in severity_outcomes:
                status_icon = {
                    CheckResult.PASS: "✓",
                    CheckResult.FAIL: "✗",
                    CheckResult.SKIP: "○"
                }[outcome.result]
                
                lines.append(f"  [{status_icon}] {outcome.check.check_id}: {outcome.check.name}")
                lines.append(f"      File: {outcome.check.file_path}")
                lines.append(f"      Status: {outcome.message}")
                lines.append(f"      Points: {outcome.check.points if outcome.result == CheckResult.PASS else 0}/{outcome.check.points}")
                lines.append("")
        
        score = self.calculate_score()
        lines.extend([
            "",
            "=" * 60,
            "SUMMARY",
            "=" * 60,
            f"  Checks Passed:  {score['passed']}/{score['total_checks']}",
            f"  Checks Failed:  {score['failed']}/{score['total_checks']}",
            f"  Checks Skipped: {score['skipped']}/{score['total_checks']}",
            "",
            f"  Points Earned:  {score['points_earned']}/{score['points_total']}",
            f"  Final Score:    {score['percentage']}%",
            "",
            "=" * 60
        ])
        
        if score['percentage'] >= 90:
            lines.append("RESULT: EXCELLENT - All critical vulnerabilities addressed!")
        elif score['percentage'] >= 70:
            lines.append("RESULT: GOOD - Most vulnerabilities addressed, some remain.")
        elif score['percentage'] >= 50:
            lines.append("RESULT: NEEDS IMPROVEMENT - Several vulnerabilities remain.")
        else:
            lines.append("RESULT: CRITICAL - Many vulnerabilities still present!")
        
        lines.append("=" * 60)
        
        return "\n".join(lines)


def main():
    """Main entry point for verification."""
    base_path = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    
    print(f"Verifying security fixes in: {base_path}")
    print()
    
    verifier = SecurityVerifier(base_path)
    verifier.run_all_checks()
    
    report = verifier.generate_report()
    print(report)
    
    score = verifier.calculate_score()
    
    if score['percentage'] < 100:
        sys.exit(1)
    
    sys.exit(0)


if __name__ == "__main__":
    main()
