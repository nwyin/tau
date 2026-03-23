In `report.py`, the same validation-and-formatting logic is repeated in both
`generate_summary` and `generate_detail`. Extract it into a helper function
called `format_entry(entry: dict) -> str` and call it from both places.

The helper should:
1. Validate the entry has `name` and `value` keys
2. Format value to 2 decimal places if it's a float
3. Return `"{name}: {formatted_value}"`

Make sure `ruff check` passes after the change.
