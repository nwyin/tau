Extract the common utility functions from `src/handlers.py` into a new `src/utils.py` module. The functions to extract are `format_response`, `validate_id`, and `parse_query`. Then:

1. Read and understand the existing code
2. Create `src/utils.py` with the extracted functions
3. Update `src/handlers.py` to import from `src/utils`
4. Create `tests/test_utils.py` with tests for each extracted function
5. Make sure the existing tests in `tests/test_handlers.py` still work

Note: pay attention to edge cases in `parse_query` -- the function should handle empty strings gracefully.