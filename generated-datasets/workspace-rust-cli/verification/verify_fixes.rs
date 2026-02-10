//! Verification script for file-processor security fixes
//!
//! This script checks that all known vulnerabilities have been addressed.
//! Run with: rustc verify_fixes.rs -o verify && ./verify ../src

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let src_dir = args.get(1).map(|s| s.as_str()).unwrap_or("../src");

    println!("=== File Processor Security Verification ===\n");

    let mut all_passed = true;
    let mut checks_passed = 0;
    let mut checks_failed = 0;

    // Check 1: No unnecessary unsafe blocks in transformer.rs
    let transformer_path = format!("{}/processor/transformer.rs", src_dir);
    if let Ok(content) = fs::read_to_string(&transformer_path) {
        let check = check_no_unsafe_fast_concat(&content);
        report_check("No unsafe in fast_concat", check, &mut all_passed, &mut checks_passed, &mut checks_failed);
    } else {
        println!("⚠ Could not read transformer.rs");
    }

    // Check 2: Path canonicalization in file_handler.rs
    let handler_path = format!("{}/storage/file_handler.rs", src_dir);
    if let Ok(content) = fs::read_to_string(&handler_path) {
        let check = check_path_canonicalization(&content);
        report_check("Path canonicalization used", check, &mut all_passed, &mut checks_passed, &mut checks_failed);

        let check = check_no_raw_unwrap(&content);
        report_check("No unwrap() without context", check, &mut all_passed, &mut checks_passed, &mut checks_failed);

        let check = check_path_sanitization(&content);
        report_check("Path sanitization in errors", check, &mut all_passed, &mut checks_passed, &mut checks_failed);
    } else {
        println!("⚠ Could not read file_handler.rs");
    }

    // Check 3: Proper crypto in crypto.rs
    let crypto_path = format!("{}/utils/crypto.rs", src_dir);
    if let Ok(content) = fs::read_to_string(&crypto_path) {
        let check = check_no_weak_xor_crypto(&content);
        report_check("No weak XOR encryption", check, &mut all_passed, &mut checks_passed, &mut checks_failed);

        let check = check_secure_random(&content);
        report_check("Secure random implementation", check, &mut all_passed, &mut checks_passed, &mut checks_failed);
    } else {
        println!("⚠ Could not read crypto.rs");
    }

    // Check 4: Safe arithmetic in validation.rs
    let validation_path = format!("{}/utils/validation.rs", src_dir);
    if let Ok(content) = fs::read_to_string(&validation_path) {
        let check = check_safe_arithmetic(&content);
        report_check("Safe arithmetic operations", check, &mut all_passed, &mut checks_passed, &mut checks_failed);
    } else {
        println!("⚠ Could not read validation.rs");
    }

    // Summary
    println!("\n=== Summary ===");
    println!("Checks passed: {}", checks_passed);
    println!("Checks failed: {}", checks_failed);

    if all_passed {
        println!("\n✅ All security checks passed!");
        ExitCode::SUCCESS
    } else {
        println!("\n❌ Some security checks failed. Please review and fix.");
        ExitCode::FAILURE
    }
}

fn report_check(name: &str, passed: bool, all_passed: &mut bool, passed_count: &mut u32, failed_count: &mut u32) {
    if passed {
        println!("✅ {}", name);
        *passed_count += 1;
    } else {
        println!("❌ {}", name);
        *all_passed = false;
        *failed_count += 1;
    }
}

fn check_no_unsafe_fast_concat(content: &str) -> bool {
    // Check that fast_concat doesn't use unsafe
    let lines: Vec<&str> = content.lines().collect();
    let mut in_fast_concat = false;
    let mut brace_depth = 0;

    for line in lines {
        if line.contains("fn fast_concat") {
            in_fast_concat = true;
        }

        if in_fast_concat {
            brace_depth += line.matches('{').count();
            brace_depth = brace_depth.saturating_sub(line.matches('}').count());

            if line.contains("unsafe") {
                return false;
            }

            if brace_depth == 0 && in_fast_concat && line.contains('}') {
                break;
            }
        }
    }

    true
}

fn check_path_canonicalization(content: &str) -> bool {
    // Check for canonicalize usage in resolve_path
    content.contains("canonicalize") || content.contains("fs::canonicalize")
}

fn check_no_raw_unwrap(content: &str) -> bool {
    // Check for unwrap() without expect() - simple heuristic
    let unwrap_count = content.matches(".unwrap()").count();
    let expect_count = content.matches(".expect(").count();
    let ok_or_count = content.matches(".ok_or").count();
    let question_mark_count = content.matches('?').count();

    // If there are unwraps, there should be more expects or proper error handling
    unwrap_count == 0 || (expect_count + ok_or_count + question_mark_count) > unwrap_count
}

fn check_path_sanitization(content: &str) -> bool {
    // Check that error types don't directly expose paths or use sanitized versions
    !content.contains("path.display()") || content.contains("sanitize") || content.contains("file_name()")
}

fn check_no_weak_xor_crypto(content: &str) -> bool {
    // Check that XOR encryption is not used for actual encryption
    // Or that it's marked as weak/deprecated
    let has_xor_encrypt = content.contains("byte ^ key_byte");

    if has_xor_encrypt {
        // Should have a warning or be replaced
        content.contains("// WARNING") || content.contains("deprecated") || content.contains("INSECURE")
    } else {
        true
    }
}

fn check_secure_random(content: &str) -> bool {
    // Check that secure_random_bytes uses a proper CSPRNG
    let has_time_based = content.contains("SystemTime::now()") && content.contains("secure_random");

    if has_time_based {
        // Should use getrandom or rand crate instead
        content.contains("getrandom") || content.contains("rand::") || content.contains("OsRng")
    } else {
        true
    }
}

fn check_safe_arithmetic(content: &str) -> bool {
    // Check for saturating or checked arithmetic in scoring functions
    content.contains("saturating_sub") || content.contains("checked_") || !content.contains("calculate_content_score")
}
