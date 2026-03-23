"""HTTP request handlers with inline utility functions."""


def format_response(data: dict, status: str = "ok") -> dict:
    """Format a standard API response."""
    return {"status": status, "data": data}


def validate_id(raw_id: str) -> int:
    """Validate and parse an ID string. Raises ValueError if invalid."""
    if not raw_id or not raw_id.strip().isdigit():
        raise ValueError(f"Invalid ID: {raw_id!r}")
    return int(raw_id.strip())


def parse_query(query_string: str) -> dict[str, str]:
    """Parse a query string like 'key1=val1&key2=val2' into a dict."""
    if not query_string:
        return {}
    pairs = query_string.split("&")
    result: dict[str, str] = {}
    for pair in pairs:
        if "=" in pair:
            key, value = pair.split("=", 1)
            result[key] = value
    return result


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
