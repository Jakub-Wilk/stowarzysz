"""Sealing and opening of a pairing while an event is active.

The token holds both ids, and opening it checks the giver id, so a row copied under another
giver is rejected rather than revealing someone else's draw.
"""

from cryptography.fernet import Fernet, InvalidToken
from django.conf import settings


def _fernet() -> Fernet:
    return Fernet(settings.SECRET_SANTA_KEY)


def seal(giver_id: int, receiver_id: int) -> str:
    return _fernet().encrypt(f"{giver_id}:{receiver_id}".encode()).decode()


def open_seal(giver_id: int, token: str) -> int:
    try:
        giver, receiver = _fernet().decrypt(token.encode()).decode().split(":")
    except (InvalidToken, ValueError) as exc:
        raise ValueError("Unreadable pairing.") from exc
    if int(giver) != giver_id:
        raise ValueError("Pairing does not belong to this giver.")
    return int(receiver)
