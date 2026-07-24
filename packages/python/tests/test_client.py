import json
import sys
import unittest
from pathlib import Path

import httpx

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from cerul import Cerul, PUBLIC_OPERATIONS


class CerulClientTest(unittest.TestCase):
    def test_generated_operation_map_drives_local_search(self) -> None:
        def handler(request: httpx.Request) -> httpx.Response:
            self.assertEqual(str(request.url), "http://127.0.0.1:23785/v1/search")
            self.assertEqual(request.headers["Authorization"], "Bearer installation_test")
            payload = json.loads(request.content)
            self.assertEqual(payload["execution_policy"], "local_only")
            return httpx.Response(
                200,
                json={
                    "request_id": "req_test1",
                    "execution": {"location": "local"},
                    "usage": {"billable": False, "quantity": 1, "unit": "query", "credits": 0},
                    "warnings": [],
                    "data": [],
                },
            )

        client = Cerul(
            token="installation_test",
            base_url="http://127.0.0.1:23785/v1",
            transport=httpx.MockTransport(handler),
        )
        try:
            response = client.search(
                "evidence",
                {"library_ids": [], "asset_ids": []},
                execution_policy="local_only",
            )
        finally:
            client.close()
        self.assertEqual(response["request_id"], "req_test1")

    def test_public_operations_include_response_and_artifact(self) -> None:
        self.assertEqual(PUBLIC_OPERATIONS["createResponse"]["path"], "/v1/responses")
        self.assertEqual(PUBLIC_OPERATIONS["getArtifact"]["path"], "/v1/artifacts/{artifact_id}")


if __name__ == "__main__":
    unittest.main()
