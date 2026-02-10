"""Database connection and utilities."""
import psycopg2
from psycopg2.extras import RealDictCursor
from contextlib import contextmanager


_connection_pool = None


def init_db(app):
    """Initialize database connection pool."""
    global _connection_pool
    
    config = app.config
    _connection_pool = {
        'host': config['DATABASE_HOST'],
        'port': config['DATABASE_PORT'],
        'database': config['DATABASE_NAME'],
        'user': config['DATABASE_USER'],
        'password': config['DATABASE_PASSWORD']
    }


def get_connection():
    """Get a database connection."""
    if _connection_pool is None:
        raise RuntimeError("Database not initialized. Call init_db first.")
    
    return psycopg2.connect(**_connection_pool)


@contextmanager
def get_cursor(commit=False):
    """Context manager for database cursor."""
    conn = get_connection()
    cursor = conn.cursor(cursor_factory=RealDictCursor)
    try:
        yield cursor
        if commit:
            conn.commit()
    except Exception as e:
        conn.rollback()
        raise e
    finally:
        cursor.close()
        conn.close()


def execute_query(query, params=None, fetch_one=False, fetch_all=False, commit=False):
    """Execute a database query."""
    with get_cursor(commit=commit) as cursor:
        cursor.execute(query, params)
        
        if fetch_one:
            return cursor.fetchone()
        elif fetch_all:
            return cursor.fetchall()
        
        return cursor.rowcount


def init_schema():
    """Initialize database schema."""
    schema = """
    CREATE TABLE IF NOT EXISTS users (
        id SERIAL PRIMARY KEY,
        username VARCHAR(50) UNIQUE NOT NULL,
        email VARCHAR(100) UNIQUE NOT NULL,
        password_hash VARCHAR(255) NOT NULL,
        role VARCHAR(20) DEFAULT 'user',
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        is_active BOOLEAN DEFAULT TRUE
    );
    
    CREATE TABLE IF NOT EXISTS sessions (
        id SERIAL PRIMARY KEY,
        user_id INTEGER REFERENCES users(id),
        session_data BYTEA,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        expires_at TIMESTAMP
    );
    
    CREATE TABLE IF NOT EXISTS api_keys (
        id SERIAL PRIMARY KEY,
        user_id INTEGER REFERENCES users(id),
        key_hash VARCHAR(255) NOT NULL,
        name VARCHAR(100),
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        last_used TIMESTAMP
    );
    
    CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
    CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
    CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
    """
    
    execute_query(schema, commit=True)
