Split the monolithic `src/config.py` into per-environment configuration files. Specifically:

1. Read and understand the current config structure
2. Create `config/base.json`, `config/dev.json`, and `config/prod.json` with the appropriate settings
3. Update `src/config.py` to load from JSON files, merging base + environment-specific
4. Create `tests/test_config.py` to verify config loading for each environment
5. Verify the tests pass