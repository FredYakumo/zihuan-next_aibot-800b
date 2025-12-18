"""
Database and data management maintenance tasks.
This module handles database migrations, ORM initialization, and data management operations.
"""

import os
import sys
from utils.config_loader import config
from utils.logging_config import logger
from database.db import engine
from database.base import Base
from alembic.config import Config as AlembicConfig
from alembic.command import upgrade as alembic_upgrade
from sqlalchemy import text


# Resolve path to alembic.ini relative to project root
BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ALEMBIC_INI_PATH = os.path.join(BASE_DIR, "alembic.ini")


def init_database():
    """Initialize the database by creating all tables."""
    logger.info("Initializing database tables...")
    Base.metadata.create_all(bind=engine)
    logger.info("Database initialization completed.")


def run_migrations():
    """Run Alembic database migrations."""
    logger.info("Running database migrations...")
    alembic_config = AlembicConfig(ALEMBIC_INI_PATH)
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
            result = conn.execute(text("SELECT 1"))
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

        run_migrations()

        init_database()

        logger.info("All maintenance tasks completed successfully.")
    except Exception as e:
        logger.error(f"Error during maintenance tasks: {str(e)}")
        sys.exit(1)


if __name__ == "__main__":
    main()
