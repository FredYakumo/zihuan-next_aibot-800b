"""make at_target_list nullable

Revision ID: e8c7d6f2b123
Revises: 6d101e418d9b
Create Date: 2025-12-19 20:05:00.000000

"""
from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision: str = "e8c7d6f2b123"
down_revision: Union[str, Sequence[str], None] = "6d101e418d9b"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """Allow at_target_list to be nullable."""
    op.alter_column(
        "message_record",
        "at_target_list",
        existing_type=sa.String(length=512),
        nullable=True,
    )


def downgrade() -> None:
    """Revert at_target_list to non-nullable."""
    op.alter_column(
        "message_record",
        "at_target_list",
        existing_type=sa.String(length=512),
        nullable=False,
    )
