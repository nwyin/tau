"""Data processor that fetches and transforms records."""


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


def process(record_ids: list[int]) -> list[dict]:
    """Process a batch of record IDs."""
    results = []
    for rid in record_ids:
        raw = fetch_record(rid)
        transformed = transform(raw)
        results.append(transformed)
    return results
