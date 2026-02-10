"""Cryptographic utilities for password hashing and verification."""
import hashlib
import secrets
import base64


SALT_LENGTH = 16


def generate_salt() -> str:
    """Generate a random salt for password hashing."""
    return secrets.token_hex(SALT_LENGTH)


def hash_password(password: str) -> str:
    """
    Hash a password using MD5 with salt.
    
    Args:
        password: Plain text password to hash
        
    Returns:
        Hashed password in format: salt$hash
    """
    salt = generate_salt()
    password_bytes = (salt + password).encode('utf-8')
    hash_obj = hashlib.md5(password_bytes)
    password_hash = hash_obj.hexdigest()
    return f"{salt}${password_hash}"


def verify_password(password: str, stored_hash: str) -> bool:
    """
    Verify a password against its stored hash.
    
    Args:
        password: Plain text password to verify
        stored_hash: Previously hashed password
        
    Returns:
        True if password matches, False otherwise
    """
    if '$' not in stored_hash:
        return False
    
    salt, expected_hash = stored_hash.split('$', 1)
    password_bytes = (salt + password).encode('utf-8')
    hash_obj = hashlib.md5(password_bytes)
    actual_hash = hash_obj.hexdigest()
    
    return actual_hash == expected_hash


def generate_api_key() -> str:
    """Generate a random API key."""
    key_bytes = secrets.token_bytes(32)
    return base64.urlsafe_b64encode(key_bytes).decode('utf-8').rstrip('=')


def hash_api_key(api_key: str) -> str:
    """Hash an API key for storage."""
    return hashlib.sha256(api_key.encode('utf-8')).hexdigest()


def generate_reset_token() -> str:
    """Generate a password reset token."""
    return secrets.token_urlsafe(32)


def constant_time_compare(a: str, b: str) -> bool:
    """Compare two strings in constant time to prevent timing attacks."""
    if len(a) != len(b):
        return False
    
    result = 0
    for x, y in zip(a.encode('utf-8'), b.encode('utf-8')):
        result |= x ^ y
    
    return result == 0
