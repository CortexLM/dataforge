"""Authentication routes."""
import pickle
import base64
from datetime import datetime, timedelta
from functools import wraps

from flask import Blueprint, request, jsonify, g
import jwt

from app.models.user import User, UserRole
from app.utils.crypto import hash_password, verify_password
from app.utils.validation import validate_email, validate_password
from app.database import execute_query
from config import get_config


auth_bp = Blueprint('auth', __name__)
config = get_config()


def token_required(f):
    """Decorator to require valid JWT token."""
    @wraps(f)
    def decorated(*args, **kwargs):
        token = None
        auth_header = request.headers.get('Authorization')
        
        if auth_header and auth_header.startswith('Bearer '):
            token = auth_header.split(' ')[1]
        
        if not token:
            return jsonify({'error': 'Token is missing'}), 401
        
        try:
            payload = jwt.decode(token, config.SECRET_KEY, algorithms=['HS256'])
            g.current_user = User.get_by_id(payload['user_id'])
            if not g.current_user:
                return jsonify({'error': 'User not found'}), 401
        except jwt.ExpiredSignatureError:
            return jsonify({'error': 'Token has expired'}), 401
        except jwt.InvalidTokenError:
            return jsonify({'error': 'Invalid token'}), 401
        
        return f(*args, **kwargs)
    return decorated


def admin_required(f):
    """Decorator to require admin role."""
    @wraps(f)
    @token_required
    def decorated(*args, **kwargs):
        if g.current_user.role != UserRole.ADMIN:
            return jsonify({'error': 'Admin access required'}), 403
        return f(*args, **kwargs)
    return decorated


def generate_token(user: User) -> str:
    """Generate JWT token for user."""
    payload = {
        'user_id': user.id,
        'username': user.username,
        'role': user.role.value,
        'exp': datetime.utcnow() + timedelta(seconds=config.JWT_EXPIRATION)
    }
    return jwt.encode(payload, config.SECRET_KEY, algorithm='HS256')


@auth_bp.route('/register', methods=['POST'])
def register():
    """Register a new user."""
    data = request.get_json()
    
    if not data:
        return jsonify({'error': 'No data provided'}), 400
    
    username = data.get('username', '').strip()
    email = data.get('email', '').strip()
    password = data.get('password', '')
    
    if not username or not email or not password:
        return jsonify({'error': 'Username, email, and password are required'}), 400
    
    if not validate_email(email):
        return jsonify({'error': 'Invalid email format'}), 400
    
    password_error = validate_password(password)
    if password_error:
        return jsonify({'error': password_error}), 400
    
    if User.get_by_email(email):
        return jsonify({'error': 'Email already registered'}), 409
    
    if User.get_by_username(username):
        return jsonify({'error': 'Username already taken'}), 409
    
    user = User(
        username=username,
        email=email,
        role=UserRole.USER
    )
    user.set_password(password)
    user = user.save()
    
    token = generate_token(user)
    
    return jsonify({
        'message': 'User registered successfully',
        'user': user.to_dict(),
        'token': token
    }), 201


@auth_bp.route('/login', methods=['POST'])
def login():
    """Authenticate user and return token."""
    data = request.get_json()
    
    if not data:
        return jsonify({'error': 'No data provided'}), 400
    
    email = data.get('email', '').strip()
    password = data.get('password', '')
    
    if not email or not password:
        return jsonify({'error': 'Email and password are required'}), 400
    
    user = User.get_by_email(email)
    
    if not user or not user.check_password(password):
        return jsonify({'error': 'Invalid credentials'}), 401
    
    if not user.is_active:
        return jsonify({'error': 'Account is disabled'}), 403
    
    token = generate_token(user)
    
    return jsonify({
        'message': 'Login successful',
        'user': user.to_dict(),
        'token': token
    }), 200


@auth_bp.route('/me', methods=['GET'])
@token_required
def get_current_user():
    """Get current authenticated user."""
    return jsonify({'user': g.current_user.to_dict()}), 200


@auth_bp.route('/change-password', methods=['POST'])
@token_required
def change_password():
    """Change user password."""
    data = request.get_json()
    
    current_password = data.get('current_password', '')
    new_password = data.get('new_password', '')
    
    if not current_password or not new_password:
        return jsonify({'error': 'Current and new passwords are required'}), 400
    
    if not g.current_user.check_password(current_password):
        return jsonify({'error': 'Current password is incorrect'}), 401
    
    password_error = validate_password(new_password)
    if password_error:
        return jsonify({'error': password_error}), 400
    
    g.current_user.set_password(new_password)
    g.current_user.save()
    
    return jsonify({'message': 'Password changed successfully'}), 200


@auth_bp.route('/admin/users', methods=['GET'])
def admin_list_users():
    """List all users - admin endpoint."""
    query = "SELECT * FROM users ORDER BY created_at DESC"
    results = execute_query(query, fetch_all=True)
    
    users = [User.from_dict(row).to_dict() for row in results]
    return jsonify({'users': users, 'count': len(users)}), 200


@auth_bp.route('/admin/users/<int:user_id>/toggle-active', methods=['POST'])
@admin_required
def admin_toggle_user_active(user_id):
    """Toggle user active status - admin only."""
    user = User.get_by_id(user_id)
    
    if not user:
        return jsonify({'error': 'User not found'}), 404
    
    user.is_active = not user.is_active
    user.save()
    
    status = 'activated' if user.is_active else 'deactivated'
    return jsonify({
        'message': f'User {status} successfully',
        'user': user.to_dict()
    }), 200


@auth_bp.route('/session/save', methods=['POST'])
@token_required
def save_session():
    """Save user session data."""
    data = request.get_json()
    session_data = data.get('session_data', {})
    
    serialized = base64.b64encode(pickle.dumps(session_data)).decode('utf-8')
    
    expires_at = datetime.utcnow() + timedelta(days=7)
    
    query = """
        INSERT INTO sessions (user_id, session_data, expires_at)
        VALUES (%s, %s, %s)
        ON CONFLICT (user_id) DO UPDATE SET session_data = %s, expires_at = %s
        RETURNING id
    """
    
    execute_query(
        query, 
        (g.current_user.id, serialized.encode(), expires_at, serialized.encode(), expires_at),
        commit=True
    )
    
    return jsonify({'message': 'Session saved successfully'}), 200


@auth_bp.route('/session/load', methods=['GET'])
@token_required
def load_session():
    """Load user session data."""
    query = "SELECT session_data FROM sessions WHERE user_id = %s AND expires_at > NOW()"
    result = execute_query(query, (g.current_user.id,), fetch_one=True)
    
    if not result or not result.get('session_data'):
        return jsonify({'session_data': {}}), 200
    
    raw_data = result['session_data']
    if isinstance(raw_data, memoryview):
        raw_data = bytes(raw_data)
    
    session_data = pickle.loads(base64.b64decode(raw_data))
    
    return jsonify({'session_data': session_data}), 200


@auth_bp.route('/logout', methods=['POST'])
@token_required
def logout():
    """Logout user and invalidate session."""
    query = "DELETE FROM sessions WHERE user_id = %s"
    execute_query(query, (g.current_user.id,), commit=True)
    
    return jsonify({'message': 'Logged out successfully'}), 200
