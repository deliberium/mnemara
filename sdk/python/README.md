# mnemara-http

Dependency-free Python HTTP client for the Mnemara daemon.

```python
from mnemara_http import MnemaraHttpClient

client = MnemaraHttpClient("http://127.0.0.1:50052", token="admin-token")

client.upsert({
    "record": {
        "id": "rec-1",
        "scope": {
            "tenant_id": "default",
            "namespace": "notes",
            "actor_id": "user",
            "source": "python-sdk",
            "labels": [],
            "trust_level": "Observed",
        },
        "kind": "Fact",
        "content": "Mnemara ships a Python HTTP SDK.",
        "metadata": {},
        "quality_state": "Active",
        "created_at_unix_ms": 0,
        "updated_at_unix_ms": 0,
        "importance_score": 0.7,
    },
    "idempotency_key": "rec-1",
})

print(client.recall({
    "scope": {
        "tenant_id": "default",
        "namespace": "notes",
        "actor_id": "user",
        "source": "python-sdk",
        "labels": [],
        "trust_level": "Observed",
    },
    "query_text": "Python SDK",
    "max_items": 3,
    "filters": {},
    "include_explanation": True,
}))
```

The client mirrors the daemon's HTTP/JSON surface, including admin graph inspection, changefeed reads, integrity checks, repair, synthesis proposals, compaction, maintenance runs, runtime status, traces, portable export/import, snapshot shipping, and `recall_as_of` time-travel recall requests.
