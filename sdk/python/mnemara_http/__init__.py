"""Dependency-free Python HTTP client for the Mnemara daemon."""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import Any, Mapping, MutableMapping
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import Request, urlopen

JsonObject = Mapping[str, Any]


class MnemaraHttpError(RuntimeError):
    """Raised when the daemon returns a non-success HTTP status."""

    def __init__(self, message: str, *, status: int, body: Any) -> None:
        super().__init__(message)
        self.status = status
        self.body = body


@dataclass(frozen=True)
class MnemaraHttpClient:
    """Small HTTP/JSON client for Mnemara's daemon API."""

    base_url: str
    token: str | None = None
    headers: Mapping[str, str] | None = None
    opener: Any = urlopen
    timeout: float | None = 30.0

    def __post_init__(self) -> None:
        if not self.base_url:
            raise ValueError("base_url is required")
        object.__setattr__(self, "base_url", self.base_url.rstrip("/"))

    def with_token(self, token: str) -> "MnemaraHttpClient":
        return MnemaraHttpClient(
            base_url=self.base_url,
            token=token,
            headers=self.headers,
            opener=self.opener,
            timeout=self.timeout,
        )

    def health(self) -> Any:
        return self._request("/healthz")

    def ready(self) -> Any:
        return self._request("/readyz")

    def metrics(self) -> str:
        return self._request_text("/metrics", accept="text/plain")

    def upsert(self, request: JsonObject) -> Any:
        return self._request("/memory/upsert", method="POST", body=request)

    def batch_upsert(self, request: JsonObject) -> Any:
        return self._request("/memory/batch-upsert", method="POST", body=request)

    def recall(self, query: JsonObject) -> Any:
        return self._request("/memory/recall", method="POST", body=query)

    def recall_as_of(self, request: JsonObject) -> Any:
        return self._request("/memory/recall-as-of", method="POST", body=request)

    def snapshot(self) -> Any:
        return self._request("/admin/snapshot")

    def stats(self, request: JsonObject | None = None) -> Any:
        return self._request("/admin/stats", query=request or {})

    def inspect_graph(self, request: JsonObject | None = None) -> Any:
        return self._request("/admin/graph", method="POST", body=request or {})

    def changefeed(self, request: JsonObject | None = None) -> Any:
        return self._request("/admin/changefeed", query=request or {})

    def integrity_check(self, request: JsonObject | None = None) -> Any:
        return self._request("/admin/integrity", query=request or {})

    def repair(self, request: JsonObject) -> Any:
        return self._request("/admin/repair", method="POST", body=request)

    def run_maintenance(self, request: JsonObject | None = None) -> Any:
        return self._request(
            "/admin/maintenance/run",
            method="POST",
            body=request or {},
        )

    def compact(self, request: JsonObject) -> Any:
        return self._request("/admin/compact", method="POST", body=request)

    def delete(self, request: JsonObject) -> Any:
        return self._request("/admin/delete", method="POST", body=request)

    def list_traces(self, request: JsonObject | None = None) -> Any:
        return self._request("/admin/traces", query=request or {})

    def get_trace(self, trace_id: str) -> Any:
        return self._request(f"/admin/traces/{trace_id}")

    def runtime_status(self) -> Any:
        return self._request("/admin/runtime")

    def export_package(self, request: JsonObject | None = None) -> Any:
        return self._request("/admin/export", method="POST", body=request or {})

    def import_package(self, request: JsonObject) -> Any:
        return self._request("/admin/import", method="POST", body=request)

    def ship_snapshot(self, request: JsonObject) -> Any:
        return self._request("/admin/replication/ship", method="POST", body=request)

    def _request(
        self,
        path: str,
        *,
        method: str = "GET",
        body: JsonObject | None = None,
        query: JsonObject | None = None,
    ) -> Any:
        text = self._request_text(path, method=method, body=body, query=query)
        if not text:
            return None
        return json.loads(text)

    def _request_text(
        self,
        path: str,
        *,
        method: str = "GET",
        body: JsonObject | None = None,
        query: JsonObject | None = None,
        accept: str = "application/json",
    ) -> str:
        url = self._url(path, query)
        headers: MutableMapping[str, str] = {
            "Accept": accept,
            **(dict(self.headers or {})),
        }
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"

        data = None
        if body is not None:
            data = json.dumps(body).encode("utf-8")
            headers["Content-Type"] = "application/json"

        request = Request(url, data=data, headers=dict(headers), method=method)
        try:
            response = self.opener(request, timeout=self.timeout)
            with response:
                return response.read().decode("utf-8")
        except HTTPError as error:
            raw = error.read().decode("utf-8")
            try:
                parsed: Any = json.loads(raw)
            except json.JSONDecodeError:
                parsed = raw
            raise MnemaraHttpError(
                f"Mnemara request failed with HTTP {error.code}",
                status=error.code,
                body=parsed,
            ) from error

    def _url(self, path: str, query: JsonObject | None) -> str:
        url = f"{self.base_url}{path}"
        if not query:
            return url
        filtered = {
            key: str(value)
            for key, value in query.items()
            if value is not None and value != ""
        }
        if not filtered:
            return url
        return f"{url}?{urlencode(filtered)}"


__all__ = ["MnemaraHttpClient", "MnemaraHttpError"]
