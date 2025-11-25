from typing import Any, Dict, List, Optional
from pydantic import BaseModel


class Sender(BaseModel):
    user_id: int
    nickname: str
    card: str
    role: Optional[str] = None


class MessageData(BaseModel):
    text: Optional[str] = None
    qq: Optional[str] = None


class MessageSegment(BaseModel):
    type: str
    data: MessageData


class TextElement(BaseModel):
    content: str
    atType: Optional[int] = None
    atUid: Optional[str] = None
    atTinyId: Optional[str] = None
    atNtUid: Optional[str] = None
    subElementType: Optional[int] = None
    atChannelId: Optional[str] = None
    linkInfo: Optional[Any] = None
    atRoleId: Optional[str] = None
    atRoleColor: Optional[int] = None
    atRoleName: Optional[str] = None
    needNotify: Optional[int] = None


class Element(BaseModel):
    elementType: int
    elementId: str
    elementGroupId: Optional[int] = None
    extBufForUI: Optional[Dict[str, Any]] = None
    textElement: Optional[TextElement] = None
    faceElement: Optional[Any] = None
    marketFaceElement: Optional[Any] = None
    replyElement: Optional[Any] = None
    picElement: Optional[Any] = None
    pttElement: Optional[Any] = None
    videoElement: Optional[Any] = None
    grayTipElement: Optional[Any] = None
    arkElement: Optional[Any] = None
    fileElement: Optional[Any] = None
    liveGiftElement: Optional[Any] = None
    markdownElement: Optional[Any] = None
    structLongMsgElement: Optional[Any] = None
    multiForwardMsgElement: Optional[Any] = None
    giphyElement: Optional[Any] = None
    walletElement: Optional[Any] = None
    inlineKeyboardElement: Optional[Any] = None
    textGiftElement: Optional[Any] = None
    calendarElement: Optional[Any] = None
    yoloGameResultElement: Optional[Any] = None
    avRecordElement: Optional[Any] = None
    structMsgElement: Optional[Any] = None
    faceBubbleElement: Optional[Any] = None
    shareLocationElement: Optional[Any] = None
    tofuRecordElement: Optional[Any] = None
    taskTopMsgElement: Optional[Any] = None
    recommendedMsgElement: Optional[Any] = None
    actionBarElement: Optional[Any] = None
    prologueMsgElement: Optional[Any] = None
    forwardMsgElement: Optional[Any] = None


class RoleInfo(BaseModel):
    roleId: str
    name: str
    color: int


class Raw(BaseModel):
    msgId: str
    msgRandom: str
    msgSeq: str
    cntSeq: str
    chatType: int
    msgType: int
    subMsgType: int
    sendType: int
    senderUid: str
    peerUid: str
    channelId: str
    guildId: str
    guildCode: str
    fromUid: str
    fromAppid: str
    msgTime: str
    msgMeta: Dict[str, Any]
    sendStatus: int
    sendRemarkName: str
    sendMemberName: str
    sendNickName: str
    guildName: str
    channelName: str
    elements: List[Element]
    auxiliaryElements: List[Any]
    records: List[Any]
    emojiLikesList: List[Any]
    commentCnt: str
    directMsgFlag: int
    directMsgMembers: List[Any]
    peerName: str
    freqLimitInfo: Optional[Any] = None
    editable: bool
    avatarMeta: str
    avatarPendant: str
    feedId: str
    roleId: str
    timeStamp: str
    clientIdentityInfo: Optional[Any] = None
    isImportMsg: bool
    atType: int
    roleType: int
    fromChannelRoleInfo: RoleInfo
    fromGuildRoleInfo: RoleInfo
    levelRoleInfo: RoleInfo
    recallTime: str
    isOnlineMsg: bool
    generalFlags: Dict[str, Any]
    clientSeq: str
    fileGroupSize: Optional[Any] = None
    foldingInfo: Optional[Any] = None
    multiTransInfo: Optional[Any] = None
    senderUin: str
    peerUin: str
    msgAttrs: Dict[str, Any]
    anonymousExtInfo: Optional[Any] = None
    nameType: int
    avatarFlag: int
    extInfoForUI: Optional[Any] = None
    personalMedal: Optional[Any] = None
    categoryManage: int
    msgEventInfo: Optional[Any] = None
    sourceType: int
    id: int


class MessageEvent(BaseModel):
    self_id: int
    user_id: int
    time: int
    message_id: int
    message_seq: int
    real_id: int
    real_seq: str
    message_type: str
    sender: Sender
    raw_message: str
    font: int
    sub_type: str
    message: List[MessageSegment]
    message_format: str
    post_type: str
    target_id: Optional[int] = None
    group_id: Optional[int] = None
    group_name: Optional[str] = None
    raw: Optional[Raw] = None


