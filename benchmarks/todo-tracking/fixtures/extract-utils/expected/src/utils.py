"""Common utility functions extracted from handlers."""


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
