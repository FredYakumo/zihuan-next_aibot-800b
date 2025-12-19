
from sqlalchemy import Column, Integer, String, DateTime
from database.base import Base

class MessageRecord(Base):
    __tablename__ = 'message_record'

    id = Column(Integer, primary_key=True, autoincrement=True)
    message_id = Column(String(64), nullable=False)
    sender_id = Column(String(64), nullable=False)
    sender_name = Column(String(128), nullable=False)
    send_time = Column(DateTime, nullable=False)
    group_id = Column(String(64), nullable=True)
    group_name = Column(String(128), nullable=True)
    content = Column(String(2048), nullable=False)
    at_target_list = Column(String(512), nullable=True)
