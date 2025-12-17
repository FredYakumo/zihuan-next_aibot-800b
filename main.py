"""
Database and data management maintenance tasks.
This script handles database migrations, ORM initialization, and data management operations.
"""

import sys
from utils.config_loader import config
from utils.logging_config import logger
from database.db import engine
from database.base import Base
from alembic.config import Config as AlembicConfig
from alembic.command import upgrade as alembic_upgrade


def init_database():
    """Initialize the database by creating all tables."""
    logger.info("Initializing database tables...")
    Base.metadata.create_all(bind=engine)
    logger.info("Database initialization completed.")


def run_migrations():
    """Run Alembic database migrations."""
    logger.info("Running database migrations...")
    alembic_config = AlembicConfig("alembic.ini")
    try:
        alembic_upgrade(alembic_config, "head")
        logger.info("Database migrations completed successfully.")
    except Exception as e:
        logger.error(f"Migration failed: {str(e)}")
        raise


def verify_database():
    """Verify database connection and accessibility."""
    logger.info("Verifying database connection...")
    try:
        with engine.connect() as conn:
            result = conn.execute("SELECT 1")
            logger.info("Database connection verified successfully.")
            return True
    except Exception as e:
        logger.error(f"Database connection failed: {str(e)}")
        return False


def main():
    """Main entry point for database and data management tasks."""
    logger.info("Starting database maintenance tasks...")
    
    try:
        # Verify database connection
        if not verify_database():
            logger.error("Cannot proceed without database connection.")
            sys.exit(1)
        
        # Run migrations
        run_migrations()
        
        # Initialize database if needed
        init_database()
        
        logger.info("All maintenance tasks completed successfully.")
    except Exception as e:
        logger.error(f"Error during maintenance tasks: {str(e)}")
        sys.exit(1)


if __name__ == "__main__":
    main()