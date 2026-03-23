"""Request handlers for the item API."""


def validate_name(data: dict) -> str:
    """Validate and return the name field from data.

    Raises ValueError if name is missing or too long.
    """
    if not data.get("name"):
        raise ValueError("Name is required")
    if len(data.get("name", "")) > 100:
        raise ValueError("Name must be 100 characters or less")
    return data["name"]


def handle_create(data: dict) -> dict:
    """Create a new item."""
    name = validate_name(data)

    item = {"id": _next_id(), "name": name, "status": "active"}
    _store(item)
    return {"ok": True, "item": item}


def handle_update(item_id: int, data: dict) -> dict:
    """Update an existing item."""
    existing = _fetch(item_id)
    if existing is None:
        raise ValueError(f"Item {item_id} not found")

    name = validate_name(data)

    existing["name"] = name
    _store(existing)
    return {"ok": True, "item": existing}


def handle_delete(item_id: int) -> dict:
    """Delete an item."""
    existing = _fetch(item_id)
    if existing is None:
        raise ValueError(f"Item {item_id} not found")
    _remove(item_id)
    return {"ok": True}


# --- storage stubs ---

_items: dict[int, dict] = {}
_counter = 0


def _next_id() -> int:
    global _counter
    _counter += 1
    return _counter


def _store(item: dict) -> None:
    _items[item["id"]] = item


def _fetch(item_id: int) -> dict | None:
    return _items.get(item_id)


def _remove(item_id: int) -> None:
    _items.pop(item_id, None)
