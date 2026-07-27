from __future__ import annotations

import os
from typing import Any, Dict, Mapping, Optional

import httpx

from .errors import CerulError
from .generated_contract import PUBLIC_OPERATIONS


class Cerul:
    """Thin synchronous client driven by the generated public operation map."""

    def __init__(
        self,
        token: Optional[str] = None,
        base_url: str = "https://api.cerul.ai/v1",
        transport: Optional[httpx.BaseTransport] = None,
    ) -> None:
        self._token = token or os.getenv("CERUL_API_KEY") or os.getenv("CERUL_INSTALLATION_TOKEN")
        self._client = httpx.Client(
            base_url=_normalize_base_url(base_url),
            transport=transport,
            timeout=30.0,
            headers={"X-Cerul-Client-Source": "sdk-python"},
        )

    def close(self) -> None:
        self._client.close()

    def request(
        self,
        operation_id: str,
        *,
        path_params: Optional[Mapping[str, str]] = None,
        json: Optional[Mapping[str, Any]] = None,
        idempotency_key: Optional[str] = None,
    ) -> Dict[str, Any]:
        operation = PUBLIC_OPERATIONS.get(operation_id)
        if operation is None:
            raise ValueError(f"unknown public operation: {operation_id}")
        route = operation["path"]
        for name, value in (path_params or {}).items():
            route = route.replace("{" + name + "}", value)
        headers: Dict[str, str] = {}
        if self._token:
            headers["Authorization"] = f"Bearer {self._token}"
        if idempotency_key:
            headers["Idempotency-Key"] = idempotency_key
        response = self._client.request(operation["method"], route, json=json, headers=headers)
        payload = _json_or_none(response)
        if not response.is_success:
            detail = payload.get("error", {}) if isinstance(payload, dict) else {}
            raise CerulError(
                response.status_code,
                str(detail.get("code", "api_error")),
                str(detail.get("message", f"Cerul API returned {response.status_code}")),
                str(payload.get("request_id")) if isinstance(payload, dict) and payload.get("request_id") else None,
            )
        return payload if isinstance(payload, dict) else {}

    def search(self, query: str, scope: Mapping[str, Any], execution_policy: str = "prefer_local") -> Dict[str, Any]:
        return self.request(
            "searchEvidence",
            json={"query": query, "scope": dict(scope), "execution_policy": execution_policy},
        )

    def create_response(
        self,
        prompt: str,
        scope: Mapping[str, Any],
        idempotency_key: str,
        execution_policy: str = "prefer_local",
    ) -> Dict[str, Any]:
        return self.request(
            "createResponse",
            json={
                "input": [{"type": "message", "role": "user", "content": prompt}],
                "scope": dict(scope),
                "execution_policy": execution_policy,
                "stream": False,
            },
            idempotency_key=idempotency_key,
        )


def _normalize_base_url(value: str) -> str:
    normalized = value.rstrip("/")
    if normalized.endswith("/v1"):
        normalized = normalized[:-3]
    return normalized


def _json_or_none(response: httpx.Response) -> Any:
    try:
        return response.json()
    except ValueError:
        return None
