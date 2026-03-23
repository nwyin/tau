Rename the module `src/helpers.py` to `src/formatting.py` and update all imports across the project. Specifically:

1. Read and understand which files import from `src/helpers`
2. Rename `src/helpers.py` to `src/formatting.py`
3. Update all import statements in `src/app.py` and `src/report.py`
4. Create `tests/test_formatting.py` with tests for the renamed module
5. Verify the existing tests still pass

Note: make sure the `truncate_text` function handles the case where max_length is 0.