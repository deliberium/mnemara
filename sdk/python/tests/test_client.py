import json
import unittest
from io import BytesIO
from urllib.error import HTTPError

from mnemara_http import MnemaraHttpClient, MnemaraHttpError


class FakeResponse:
    def __init__(self, payload):
        self.payload = payload

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def read(self):
        return self.payload


class ClientTests(unittest.TestCase):
    def test_posts_json_and_auth_header(self):
        seen = {}

        def opener(request, timeout=None):
            seen["url"] = request.full_url
            seen["method"] = request.get_method()
            seen["auth"] = request.headers["Authorization"]
            seen["body"] = json.loads(request.data.decode("utf-8"))
            seen["timeout"] = timeout
            return FakeResponse(b'{"ok": true}')

        client = MnemaraHttpClient(
            "http://127.0.0.1:50052/",
            token="secret",
            opener=opener,
            timeout=3.0,
        )
        self.assertEqual(client.run_maintenance({"dry_run": True}), {"ok": True})
        self.assertEqual(seen["url"], "http://127.0.0.1:50052/admin/maintenance/run")
        self.assertEqual(seen["method"], "POST")
        self.assertEqual(seen["auth"], "Bearer secret")
        self.assertEqual(seen["body"], {"dry_run": True})
        self.assertEqual(seen["timeout"], 3.0)

    def test_query_filters_empty_values(self):
        seen = {}

        def opener(request, timeout=None):
            seen["url"] = request.full_url
            return FakeResponse(b"[]")

        client = MnemaraHttpClient("http://daemon", opener=opener)
        self.assertEqual(
            client.list_traces({"tenant_id": "default", "namespace": None, "limit": 5}),
            [],
        )
        self.assertEqual(seen["url"], "http://daemon/admin/traces?tenant_id=default&limit=5")

    def test_http_error_preserves_json_body(self):
        def opener(request, timeout=None):
            raise HTTPError(
                request.full_url,
                401,
                "Unauthorized",
                hdrs={},
                fp=BytesIO(b'{"error": "nope"}'),
            )

        client = MnemaraHttpClient("http://daemon", opener=opener)
        with self.assertRaises(MnemaraHttpError) as raised:
            client.health()
        self.assertEqual(raised.exception.status, 401)
        self.assertEqual(raised.exception.body, {"error": "nope"})


if __name__ == "__main__":
    unittest.main()
