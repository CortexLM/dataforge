"""Flask application factory."""
from flask import Flask
from config import get_config


def create_app(config_name=None):
    """Create and configure the Flask application."""
    app = Flask(__name__)
    
    config = get_config()
    app.config.from_object(config)
    
    # Initialize database
    from app.database import init_db
    init_db(app)
    
    # Register blueprints
    from app.routes.auth import auth_bp
    from app.routes.users import users_bp
    
    app.register_blueprint(auth_bp, url_prefix='/api/auth')
    app.register_blueprint(users_bp, url_prefix='/api/users')
    
    @app.route('/health')
    def health_check():
        return {'status': 'healthy', 'version': '1.0.0'}
    
    return app
