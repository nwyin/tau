"""Data processor that fetches and transforms records."""

from src.cache import LRUCache


def fetch_record(record_id: int) -> dict:
    """Simulate fetching a record from a slow data source."""
    # In production this would be a database or API call
    return {"id": record_id, "value": record_id * 10, "status": "active"}


def transform(record: dict) -> dict:
    """Apply business logic transformation."""
    return {
        "id": record["id"],
        "computed": record["value"] * 2 + 1,
        "label": f"item-{record['id']}",
    }


def process(record_ids: list[int], cache: LRUCache | None = None) -> list[dict]:
    """Process a batch of record IDs, using cache if provided."""
    results = []
    for rid in record_ids:
        cache_key = str(rid)
        raw = None
        if cache is not None:
            raw = cache.get(cache_key)
        if raw is None:
            raw = fetch_record(rid)
            if cache is not None:
                cache.put(cache_key, raw)
        transformed = transform(raw)
        results.append(transformed)
    return results
