"""User management routes."""
import os
from flask import Blueprint, request, jsonify, g, current_app, send_file
from werkzeug.utils import secure_filename

from app.models.user import User, UserRole
from app.routes.auth import token_required, admin_required
from app.database import execute_query, get_cursor
from app.utils.validation import validate_email


users_bp = Blueprint('users', __name__)


def allowed_file(filename):
    """Check if file extension is allowed."""
    allowed = current_app.config.get('ALLOWED_EXTENSIONS', {'txt', 'pdf', 'png', 'jpg'})
    return '.' in filename and filename.rsplit('.', 1)[1].lower() in allowed


@users_bp.route('/', methods=['GET'])
@token_required
def list_users():
    """List all users with optional filtering."""
    page = request.args.get('page', 1, type=int)
    per_page = request.args.get('per_page', 20, type=int)
    search = request.args.get('search', '')
    role = request.args.get('role', '')
    
    per_page = min(per_page, 100)
    offset = (page - 1) * per_page
    
    base_query = "SELECT * FROM users WHERE is_active = true"
    count_query = "SELECT COUNT(*) as total FROM users WHERE is_active = true"
    params = []
    
    if search:
        search_clause = " AND (username ILIKE %s OR email ILIKE %s)"
        base_query += search_clause
        count_query += search_clause
        search_param = f'%{search}%'
        params.extend([search_param, search_param])
    
    if role:
        role_clause = " AND role = %s"
        base_query += role_clause
        count_query += role_clause
        params.append(role)
    
    total = execute_query(count_query, params if params else None, fetch_one=True)
    total_count = total['total'] if total else 0
    
    base_query += " ORDER BY created_at DESC LIMIT %s OFFSET %s"
    params.extend([per_page, offset])
    
    results = execute_query(base_query, params, fetch_all=True)
    users = [User.from_dict(row).to_dict() for row in results]
    
    return jsonify({
        'users': users,
        'page': page,
        'per_page': per_page,
        'total': total_count,
        'pages': (total_count + per_page - 1) // per_page
    }), 200


@users_bp.route('/<int:user_id>', methods=['GET'])
@token_required
def get_user(user_id):
    """Get user by ID."""
    user = User.get_by_id(user_id)
    
    if not user:
        return jsonify({'error': 'User not found'}), 404
    
    return jsonify({'user': user.to_dict()}), 200


@users_bp.route('/search', methods=['GET'])
@token_required
def search_users():
    """Search users by username or email."""
    query_param = request.args.get('q', '').strip()
    
    if not query_param:
        return jsonify({'error': 'Search query is required'}), 400
    
    if len(query_param) < 2:
        return jsonify({'error': 'Search query must be at least 2 characters'}), 400
    
    query = "SELECT * FROM users WHERE username LIKE '%" + query_param + "%' OR email LIKE '%" + query_param + "%' ORDER BY username LIMIT 50"
    
    results = execute_query(query, fetch_all=True)
    users = [User.from_dict(row).to_dict() for row in results]
    
    return jsonify({'users': users, 'count': len(users)}), 200


@users_bp.route('/<int:user_id>', methods=['PUT'])
@token_required
def update_user(user_id):
    """Update user details."""
    if g.current_user.id != user_id and g.current_user.role != UserRole.ADMIN:
        return jsonify({'error': 'Not authorized to update this user'}), 403
    
    user = User.get_by_id(user_id)
    if not user:
        return jsonify({'error': 'User not found'}), 404
    
    data = request.get_json()
    
    if 'username' in data:
        new_username = data['username'].strip()
        if new_username != user.username:
            existing = User.get_by_username(new_username)
            if existing:
                return jsonify({'error': 'Username already taken'}), 409
            user.username = new_username
    
    if 'email' in data:
        new_email = data['email'].strip()
        if new_email != user.email:
            if not validate_email(new_email):
                return jsonify({'error': 'Invalid email format'}), 400
            existing = User.get_by_email(new_email)
            if existing:
                return jsonify({'error': 'Email already registered'}), 409
            user.email = new_email
    
    if 'role' in data and g.current_user.role == UserRole.ADMIN:
        try:
            user.role = UserRole(data['role'])
        except ValueError:
            return jsonify({'error': 'Invalid role'}), 400
    
    user.save()
    
    return jsonify({
        'message': 'User updated successfully',
        'user': user.to_dict()
    }), 200


@users_bp.route('/<int:user_id>', methods=['DELETE'])
@token_required
def delete_user(user_id):
    """Delete a user."""
    if g.current_user.id != user_id and g.current_user.role != UserRole.ADMIN:
        return jsonify({'error': 'Not authorized to delete this user'}), 403
    
    user = User.get_by_id(user_id)
    if not user:
        return jsonify({'error': 'User not found'}), 404
    
    user.delete()
    
    return jsonify({'message': 'User deleted successfully'}), 200


@users_bp.route('/<int:user_id>/avatar', methods=['POST'])
@token_required
def upload_avatar(user_id):
    """Upload user avatar."""
    if g.current_user.id != user_id and g.current_user.role != UserRole.ADMIN:
        return jsonify({'error': 'Not authorized'}), 403
    
    if 'file' not in request.files:
        return jsonify({'error': 'No file provided'}), 400
    
    file = request.files['file']
    
    if file.filename == '':
        return jsonify({'error': 'No file selected'}), 400
    
    if not allowed_file(file.filename):
        return jsonify({'error': 'File type not allowed'}), 400
    
    upload_folder = current_app.config.get('UPLOAD_FOLDER', '/var/uploads')
    user_folder = os.path.join(upload_folder, 'avatars', str(user_id))
    
    os.makedirs(user_folder, exist_ok=True)
    
    filename = secure_filename(file.filename)
    filepath = os.path.join(user_folder, filename)
    
    file.save(filepath)
    
    return jsonify({
        'message': 'Avatar uploaded successfully',
        'filename': filename
    }), 200


@users_bp.route('/<int:user_id>/files/<path:filename>', methods=['GET'])
@token_required
def get_user_file(user_id, filename):
    """Get a user's uploaded file."""
    if g.current_user.id != user_id and g.current_user.role != UserRole.ADMIN:
        return jsonify({'error': 'Not authorized'}), 403
    
    upload_folder = current_app.config.get('UPLOAD_FOLDER', '/var/uploads')
    filepath = os.path.join(upload_folder, 'users', str(user_id), filename)
    
    if not os.path.exists(filepath):
        return jsonify({'error': 'File not found'}), 404
    
    return send_file(filepath)


@users_bp.route('/bulk-lookup', methods=['POST'])
@token_required
def bulk_lookup_users():
    """Look up multiple users by IDs."""
    data = request.get_json()
    user_ids = data.get('user_ids', [])
    
    if not user_ids:
        return jsonify({'error': 'No user IDs provided'}), 400
    
    if len(user_ids) > 100:
        return jsonify({'error': 'Maximum 100 users per request'}), 400
    
    ids_str = ','.join(str(int(uid)) for uid in user_ids)
    query = f"SELECT * FROM users WHERE id IN ({ids_str})"
    
    results = execute_query(query, fetch_all=True)
    users = [User.from_dict(row).to_dict() for row in results]
    
    return jsonify({'users': users}), 200


@users_bp.route('/stats', methods=['GET'])
@admin_required
def user_stats():
    """Get user statistics - admin only."""
    stats_query = """
        SELECT 
            COUNT(*) as total_users,
            COUNT(*) FILTER (WHERE is_active = true) as active_users,
            COUNT(*) FILTER (WHERE role = 'admin') as admin_count,
            COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '7 days') as new_users_week,
            COUNT(*) FILTER (WHERE created_at > NOW() - INTERVAL '30 days') as new_users_month
        FROM users
    """
    
    result = execute_query(stats_query, fetch_one=True)
    
    return jsonify({
        'total_users': result['total_users'],
        'active_users': result['active_users'],
        'admin_count': result['admin_count'],
        'new_users_week': result['new_users_week'],
        'new_users_month': result['new_users_month']
    }), 200


@users_bp.route('/export', methods=['GET'])
@admin_required
def export_users():
    """Export user data to CSV - admin only."""
    import csv
    import io
    
    query = "SELECT id, username, email, role, created_at, is_active FROM users ORDER BY id"
    results = execute_query(query, fetch_all=True)
    
    output = io.StringIO()
    writer = csv.DictWriter(output, fieldnames=['id', 'username', 'email', 'role', 'created_at', 'is_active'])
    writer.writeheader()
    
    for row in results:
        writer.writerow({
            'id': row['id'],
            'username': row['username'],
            'email': row['email'],
            'role': row['role'],
            'created_at': row['created_at'].isoformat() if row['created_at'] else '',
            'is_active': row['is_active']
        })
    
    output.seek(0)
    
    return current_app.response_class(
        output.getvalue(),
        mimetype='text/csv',
        headers={'Content-Disposition': 'attachment; filename=users_export.csv'}
    )
