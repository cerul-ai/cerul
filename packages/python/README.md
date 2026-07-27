# Cerul Python SDK

The Python SDK uses the operation registry generated from Cerul's public
OpenAPI projection.

```python
from cerul import Cerul

client = Cerul(token="...", base_url="https://api.cerul.ai/v1")
result = client.search(
    "grounded evidence",
    {"library_ids": ["library_demo1"], "asset_ids": []},
)
client.close()
```

Use the same class with a local loopback `/v1` URL and installation token.
