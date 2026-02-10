"""API routes package."""
from app.routes.auth import auth_bp
from app.routes.users import users_bp

__all__ = ['auth_bp', 'users_bp']
