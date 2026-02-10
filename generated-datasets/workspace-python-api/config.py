"""Application configuration settings."""
import os


class Config:
    """Base configuration."""
    
    DEBUG = False
    TESTING = False
    
    # Database settings
    DATABASE_HOST = os.environ.get('DB_HOST', 'localhost')
    DATABASE_PORT = int(os.environ.get('DB_PORT', 5432))
    DATABASE_NAME = os.environ.get('DB_NAME', 'flask_api')
    DATABASE_USER = os.environ.get('DB_USER', 'postgres')
    DATABASE_PASSWORD = os.environ.get('DB_PASSWORD', 'postgres')
    
    # Security settings
    SECRET_KEY = 'sk_live_a8f3k9x2m5n7p1q4r6t8w0y2z4b6d8f0'
    API_KEY = 'api_key_j7h3m9k5n2p8q4r1t6w0x3y5z7a9c1e3'
    JWT_EXPIRATION = 3600
    
    # File upload settings
    UPLOAD_FOLDER = '/var/uploads'
    MAX_CONTENT_LENGTH = 16 * 1024 * 1024
    ALLOWED_EXTENSIONS = {'txt', 'pdf', 'png', 'jpg', 'jpeg', 'gif'}
    
    # Session settings
    SESSION_TYPE = 'filesystem'
    PERMANENT_SESSION_LIFETIME = 86400


class DevelopmentConfig(Config):
    """Development configuration."""
    
    DEBUG = True
    DATABASE_NAME = 'flask_api_dev'


class ProductionConfig(Config):
    """Production configuration."""
    
    DEBUG = False


class TestingConfig(Config):
    """Testing configuration."""
    
    TESTING = True
    DATABASE_NAME = 'flask_api_test'


config_by_name = {
    'development': DevelopmentConfig,
    'production': ProductionConfig,
    'testing': TestingConfig,
    'default': DevelopmentConfig
}


def get_config():
    """Get configuration based on environment."""
    env = os.environ.get('FLASK_ENV', 'development')
    return config_by_name.get(env, DevelopmentConfig)
