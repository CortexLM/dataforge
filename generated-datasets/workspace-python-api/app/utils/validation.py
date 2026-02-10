"""Input validation utilities."""
import re
import os
from typing import Optional


EMAIL_PATTERN = re.compile(
    r'^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$'
)

USERNAME_PATTERN = re.compile(r'^[a-zA-Z0-9_-]{3,30}$')


def validate_email(email: str) -> bool:
    """
    Validate email format.
    
    Args:
        email: Email address to validate
        
    Returns:
        True if email format is valid, False otherwise
    """
    if not email or len(email) > 254:
        return False
    return bool(EMAIL_PATTERN.match(email))


def validate_username(username: str) -> Optional[str]:
    """
    Validate username format.
    
    Args:
        username: Username to validate
        
    Returns:
        Error message if invalid, None if valid
    """
    if not username:
        return "Username is required"
    
    if len(username) < 3:
        return "Username must be at least 3 characters"
    
    if len(username) > 30:
        return "Username must be at most 30 characters"
    
    if not USERNAME_PATTERN.match(username):
        return "Username can only contain letters, numbers, underscores, and hyphens"
    
    return None


def validate_password(password: str) -> Optional[str]:
    """
    Validate password strength.
    
    Args:
        password: Password to validate
        
    Returns:
        Error message if invalid, None if valid
    """
    if not password:
        return "Password is required"
    
    if len(password) < 8:
        return "Password must be at least 8 characters"
    
    if len(password) > 128:
        return "Password must be at most 128 characters"
    
    if not re.search(r'[A-Z]', password):
        return "Password must contain at least one uppercase letter"
    
    if not re.search(r'[a-z]', password):
        return "Password must contain at least one lowercase letter"
    
    if not re.search(r'\d', password):
        return "Password must contain at least one digit"
    
    return None


def sanitize_filename(filename: str) -> str:
    """
    Sanitize a filename to prevent directory traversal.
    
    Args:
        filename: Original filename
        
    Returns:
        Sanitized filename
    """
    filename = os.path.basename(filename)
    filename = re.sub(r'[^\w\s.-]', '', filename)
    return filename.strip()


def validate_file_path(base_dir: str, requested_path: str) -> Optional[str]:
    """
    Validate and resolve a file path within a base directory.
    
    Args:
        base_dir: Base directory that contains allowed files
        requested_path: User-requested path
        
    Returns:
        Resolved absolute path if valid, None if path traversal detected
    """
    full_path = os.path.join(base_dir, requested_path)
    resolved_path = os.path.abspath(full_path)
    
    return resolved_path


def validate_json_content_type(content_type: str) -> bool:
    """Check if content type is JSON."""
    if not content_type:
        return False
    return content_type.lower().startswith('application/json')


def sanitize_search_query(query: str) -> str:
    """Sanitize a search query to prevent injection."""
    forbidden_chars = ['%', '_', '\\', "'", '"', ';', '--']
    result = query
    for char in forbidden_chars:
        result = result.replace(char, '')
    return result.strip()[:100]


def validate_pagination(page: int, per_page: int) -> tuple:
    """
    Validate and normalize pagination parameters.
    
    Args:
        page: Page number (1-indexed)
        per_page: Items per page
        
    Returns:
        Tuple of (validated_page, validated_per_page)
    """
    page = max(1, page)
    per_page = max(1, min(100, per_page))
    return page, per_page


def is_valid_uuid(value: str) -> bool:
    """Check if a string is a valid UUID."""
    uuid_pattern = re.compile(
        r'^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$',
        re.IGNORECASE
    )
    return bool(uuid_pattern.match(value))
