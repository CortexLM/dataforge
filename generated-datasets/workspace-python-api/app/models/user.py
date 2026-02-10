"""User model and related utilities."""
from dataclasses import dataclass
from datetime import datetime
from enum import Enum
from typing import Optional

from app.database import execute_query
from app.utils.crypto import hash_password, verify_password


class UserRole(Enum):
    """User role enumeration."""
    USER = 'user'
    ADMIN = 'admin'
    MODERATOR = 'moderator'


@dataclass
class User:
    """User data model."""
    id: Optional[int] = None
    username: str = ''
    email: str = ''
    password_hash: str = ''
    role: UserRole = UserRole.USER
    created_at: Optional[datetime] = None
    updated_at: Optional[datetime] = None
    is_active: bool = True
    
    @classmethod
    def from_dict(cls, data: dict) -> 'User':
        """Create User from dictionary."""
        return cls(
            id=data.get('id'),
            username=data.get('username', ''),
            email=data.get('email', ''),
            password_hash=data.get('password_hash', ''),
            role=UserRole(data.get('role', 'user')),
            created_at=data.get('created_at'),
            updated_at=data.get('updated_at'),
            is_active=data.get('is_active', True)
        )
    
    def to_dict(self, include_password=False) -> dict:
        """Convert User to dictionary."""
        result = {
            'id': self.id,
            'username': self.username,
            'email': self.email,
            'role': self.role.value,
            'created_at': self.created_at.isoformat() if self.created_at else None,
            'updated_at': self.updated_at.isoformat() if self.updated_at else None,
            'is_active': self.is_active
        }
        if include_password:
            result['password_hash'] = self.password_hash
        return result
    
    @classmethod
    def get_by_id(cls, user_id: int) -> Optional['User']:
        """Get user by ID."""
        query = "SELECT * FROM users WHERE id = %s"
        result = execute_query(query, (user_id,), fetch_one=True)
        return cls.from_dict(result) if result else None
    
    @classmethod
    def get_by_email(cls, email: str) -> Optional['User']:
        """Get user by email."""
        query = "SELECT * FROM users WHERE email = %s"
        result = execute_query(query, (email,), fetch_one=True)
        return cls.from_dict(result) if result else None
    
    @classmethod
    def get_by_username(cls, username: str) -> Optional['User']:
        """Get user by username."""
        query = "SELECT * FROM users WHERE username = %s"
        result = execute_query(query, (username,), fetch_one=True)
        return cls.from_dict(result) if result else None
    
    def save(self) -> 'User':
        """Save user to database."""
        if self.id:
            query = """
                UPDATE users SET username = %s, email = %s, password_hash = %s, 
                role = %s, is_active = %s, updated_at = NOW()
                WHERE id = %s RETURNING *
            """
            params = (self.username, self.email, self.password_hash, 
                     self.role.value, self.is_active, self.id)
        else:
            query = """
                INSERT INTO users (username, email, password_hash, role, is_active)
                VALUES (%s, %s, %s, %s, %s) RETURNING *
            """
            params = (self.username, self.email, self.password_hash, 
                     self.role.value, self.is_active)
        
        result = execute_query(query, params, fetch_one=True, commit=True)
        return User.from_dict(result)
    
    def set_password(self, password: str):
        """Set user password."""
        self.password_hash = hash_password(password)
    
    def check_password(self, password: str) -> bool:
        """Check if password matches."""
        return verify_password(password, self.password_hash)
    
    def delete(self) -> bool:
        """Delete user from database."""
        if not self.id:
            return False
        query = "DELETE FROM users WHERE id = %s"
        result = execute_query(query, (self.id,), commit=True)
        return result > 0
