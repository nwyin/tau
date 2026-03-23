"""HTTP request handlers using extracted utility functions."""

from src.utils import format_response, parse_query, validate_id


def handle_get(item_id_str: str) -> dict:
    """Handle GET request for an item."""
    item_id = validate_id(item_id_str)
    item = {"id": item_id, "name": f"Item {item_id}"}
    return format_response(item)


def handle_list(query_string: str) -> dict:
    """Handle GET request for item listing."""
    params = parse_query(query_string)
    limit = int(params.get("limit", "10"))
    items = [{"id": i, "name": f"Item {i}"} for i in range(1, limit + 1)]
    return format_response({"items": items, "total": len(items)})


def handle_create(data: dict) -> dict:
    """Handle POST request to create an item."""
    if "name" not in data:
        return format_response({"error": "name required"}, status="error")
    item = {"id": 99, "name": data["name"]}
    return format_response(item, status="created")
