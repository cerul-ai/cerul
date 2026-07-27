from __future__ import annotations

from typing import Optional


class CerulError(Exception):
    """Error raised for Cerul API and transport failures."""

    def __init__(self, status_code: int, code: str, message: str, request_id: Optional[str] = None) -> None:
        super().__init__(message)
        self.status_code = status_code
        self.code = code
        self.message = message
        self.request_id = request_id

    def __str__(self) -> str:
        return self.message
