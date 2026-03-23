#!/usr/bin/env python3
"""Generate synthetic multi-turn coding conversations with planted facts.

Each conversation simulates a realistic 50-turn coding session (explore,
define functions, encounter errors, make decisions).  5-8 facts are planted
at specific turns so that recall can be measured after compaction.

Usage:
    python generate.py --conversations 10 --facts-per 6 -o fixtures/
    python generate.py --conversations 5 --facts-per 8 -o fixtures/ --seed 42
"""

from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Planted-fact pool
# ---------------------------------------------------------------------------

FACT_POOL: list[dict] = [
    # function-names
    {
        "category": "function-names",
        "fact": "Defined process_batch(), validate_input(), format_output()",
        "recall_question": "What were the 3 helper functions we defined earlier?",
        "expected_answer_contains": ["process_batch", "validate_input", "format_output"],
    },
    {
        "category": "function-names",
        "fact": "Created load_config(), parse_args(), and setup_logging()",
        "recall_question": "What helper functions did we create for the CLI setup?",
        "expected_answer_contains": ["load_config", "parse_args", "setup_logging"],
    },
    {
        "category": "function-names",
        "fact": "Implemented serialize_record(), deserialize_record(), and merge_records()",
        "recall_question": "What serialization-related functions did we write?",
        "expected_answer_contains": ["serialize_record", "deserialize_record", "merge_records"],
    },
    {
        "category": "function-names",
        "fact": "Added retry_with_backoff() and is_transient_error() for resilience",
        "recall_question": "What functions did we add for error resilience?",
        "expected_answer_contains": ["retry_with_backoff", "is_transient_error"],
    },
    # error-messages
    {
        "category": "error-messages",
        "fact": "Got TypeError: 'NoneType' object is not subscriptable on line 47 of parser.py",
        "recall_question": "What error did we encounter in parser.py?",
        "expected_answer_contains": ["TypeError", "NoneType", "not subscriptable"],
    },
    {
        "category": "error-messages",
        "fact": "Hit KeyError: 'user_id' when processing empty API response",
        "recall_question": "What KeyError did we encounter with the API response?",
        "expected_answer_contains": ["KeyError", "user_id"],
    },
    {
        "category": "error-messages",
        "fact": "Saw ImportError: cannot import name 'DataProcessor' from 'core.engine'",
        "recall_question": "What ImportError did we hit?",
        "expected_answer_contains": ["ImportError", "DataProcessor", "core.engine"],
    },
    {
        "category": "error-messages",
        "fact": "Got ConnectionRefusedError on port 5432 because PostgreSQL was not running",
        "recall_question": "What connection error did we encounter and why?",
        "expected_answer_contains": ["ConnectionRefusedError", "5432"],
    },
    # design-decisions
    {
        "category": "design-decisions",
        "fact": "Chose HashMap over BTreeMap because lookup speed matters more than ordering",
        "recall_question": "Why did we choose HashMap over BTreeMap?",
        "expected_answer_contains": ["HashMap", "lookup speed"],
    },
    {
        "category": "design-decisions",
        "fact": "Decided to use SQLite instead of PostgreSQL to keep deployment simple",
        "recall_question": "Why did we pick SQLite over PostgreSQL?",
        "expected_answer_contains": ["SQLite", "deployment simple"],
    },
    {
        "category": "design-decisions",
        "fact": "Chose dataclasses over Pydantic because we want zero external dependencies",
        "recall_question": "Why did we use dataclasses instead of Pydantic?",
        "expected_answer_contains": ["dataclasses", "zero external dependencies"],
    },
    {
        "category": "design-decisions",
        "fact": "Picked TOML over YAML for configuration because it is part of the stdlib since 3.11",
        "recall_question": "Why did we choose TOML for configuration?",
        "expected_answer_contains": ["TOML", "stdlib"],
    },
    # file-paths
    {
        "category": "file-paths",
        "fact": "Modified src/api/routes.py, src/api/middleware.py, and tests/test_routes.py",
        "recall_question": "Which files did we modify during the API changes?",
        "expected_answer_contains": ["src/api/routes.py", "src/api/middleware.py", "tests/test_routes.py"],
    },
    {
        "category": "file-paths",
        "fact": "Created src/utils/cache.py and updated src/main.py to import it",
        "recall_question": "What files did we create and update for the caching work?",
        "expected_answer_contains": ["src/utils/cache.py", "src/main.py"],
    },
    {
        "category": "file-paths",
        "fact": "Edited config/settings.toml, config/logging.toml, and src/config_loader.py",
        "recall_question": "Which config-related files did we edit?",
        "expected_answer_contains": ["config/settings.toml", "config/logging.toml", "src/config_loader.py"],
    },
    # constraints
    {
        "category": "constraints",
        "fact": "User specified maximum response time must be under 200ms for the health endpoint",
        "recall_question": "What performance constraint did the user specify for the health endpoint?",
        "expected_answer_contains": ["200ms", "health endpoint"],
    },
    {
        "category": "constraints",
        "fact": "Must maintain backward compatibility with the v1 REST API for 6 months",
        "recall_question": "What backward compatibility constraint was mentioned?",
        "expected_answer_contains": ["backward compatibility", "v1"],
    },
    {
        "category": "constraints",
        "fact": "Memory usage must stay below 512MB because the container limit is set there",
        "recall_question": "What was the memory constraint mentioned?",
        "expected_answer_contains": ["512MB"],
    },
    {
        "category": "constraints",
        "fact": "The solution must work on Python 3.10+ since production is still on 3.10",
        "recall_question": "What Python version constraint was given?",
        "expected_answer_contains": ["3.10"],
    },
    # rejected-alternatives
    {
        "category": "rejected-alternatives",
        "fact": "Rejected using Redis for caching because it adds an infrastructure dependency",
        "recall_question": "What caching approach did we reject and why?",
        "expected_answer_contains": ["Redis", "infrastructure dependency"],
    },
    {
        "category": "rejected-alternatives",
        "fact": "Considered asyncio but rejected it because the IO is CPU-bound not network-bound",
        "recall_question": "Why did we reject the asyncio approach?",
        "expected_answer_contains": ["asyncio", "CPU-bound"],
    },
    {
        "category": "rejected-alternatives",
        "fact": "Rejected a microservice split because the team is too small to maintain separate deploys",
        "recall_question": "Why did we reject splitting into microservices?",
        "expected_answer_contains": ["microservice", "too small"],
    },
    {
        "category": "rejected-alternatives",
        "fact": "Considered using celery for task queuing but rejected it because a simple threading pool suffices",
        "recall_question": "Why did we reject celery for background tasks?",
        "expected_answer_contains": ["celery", "threading pool"],
    },
]


# ---------------------------------------------------------------------------
# Conversation templates
# ---------------------------------------------------------------------------
# Each template is a list of (turn_number, role, content_template) triples.
# Slots like {FACT_N} are replaced with planted facts at generation time.
# Templates cover turns 1-50 with fact insertion points and a compaction
# trigger point around turn 31.
# ---------------------------------------------------------------------------


def _build_template_explore_and_refactor() -> list[tuple[int, str, str]]:
    """Template 1: explore a project, refactor a module, add tests."""
    return [
        (1, "user", "Let's start by exploring the project structure. Show me the directory layout."),
        (
            1,
            "assistant",
            "I'll look at the project structure.\n\n```\nsrc/\n  main.py\n  api/\n    routes.py\n    middleware.py\n  utils/\n    helpers.py\n  models/\n    user.py\ntests/\n  test_routes.py\nconfig/\n  settings.toml\n```\n\nThe project has a standard layout with source in `src/`, tests, and config.",
        ),
        (2, "user", "Read src/main.py and tell me what it does."),
        (
            2,
            "assistant",
            "I'll read the file.\n\n`src/main.py` is the entry point. It imports routes from `src/api/routes.py`, sets up logging, and starts the ASGI server on port 8000. There's also a health check endpoint at `/health`.",
        ),
        (3, "user", "Now read src/api/routes.py."),
        (
            3,
            "assistant",
            "The routes file defines 4 endpoints: `GET /users`, `POST /users`, `GET /users/{id}`, and `DELETE /users/{id}`. Each handler is about 20 lines with inline validation logic and database queries.",
        ),
        (4, "user", "There's a lot of duplicated validation. Let's refactor."),
        (
            4,
            "assistant",
            "Agreed. I see repeated input validation in `POST /users` and `GET /users/{id}`. Let me plan the refactoring:\n1. Extract validation into helper functions\n2. Create a utils module for shared logic\n3. Update the routes to use the helpers\n4. Add tests for the new helpers",
        ),
        (5, "user", "Sounds good. Start with step 1 - define the helper functions."),
        (5, "assistant", "I'll create the helper functions now."),
        # Fact insertion point 1 (turn 6-7)
        (6, "user", "What functions are you defining?"),
        (6, "assistant", "{FACT_0}. These will handle the common validation and transformation patterns I found in the routes."),
        (7, "user", "Good, now implement them."),
        (
            7,
            "assistant",
            "I've implemented all three. Each takes the raw input dict and returns a validated/transformed result or raises a ValueError with a descriptive message.",
        ),
        (8, "user", "Let's test them manually first. Try calling them with sample data."),
        (
            8,
            "assistant",
            "Running quick tests:\n- Function 1: works with valid input\n- Function 2: correctly rejects empty strings\n- Function 3: handles the edge case of None values",
        ),
        (9, "user", "Good. Now let's update the routes to use these helpers."),
        (
            9,
            "assistant",
            "I'll update `src/api/routes.py` to import and use the new helpers. This reduces each handler from ~20 lines to ~8 lines.",
        ),
        (10, "user", "Wait, I'm getting an error when running the server."),
        # Fact insertion point 2 (turn 11-12)
        (11, "user", "Here's the traceback."),
        (
            11,
            "assistant",
            "{FACT_1}. The issue is that we're trying to access a key on a response object that can be None when the API returns an empty body.",
        ),
        (12, "user", "How should we fix it?"),
        (
            12,
            "assistant",
            "We need to add a None check before accessing the response data. I'll add a guard clause at the top of the function.",
        ),
        (13, "user", "Done. Now let's think about the overall architecture."),
        # Fact insertion point 3 (turn 14-15)
        (14, "user", "Should we use a specific data structure for the lookup table?"),
        (14, "assistant", "{FACT_2}. For our use case with ~10K entries and frequent random access, hash-based lookup is the right choice."),
        (15, "user", "The user also mentioned some requirements we need to keep in mind."),
        # Fact insertion point 4 (turn 15-16)
        (15, "assistant", "Yes, noted. {FACT_3}. We should make sure our changes respect that."),
        (16, "user", "Right. Let me also tell you about some files we need to change."),
        # Fact insertion point 5 (turn 17)
        (17, "user", "Here's what we need to modify."),
        (17, "assistant", "Understood. {FACT_4}. I'll work through them one at a time."),
        (18, "user", "Start with the first file."),
        (18, "assistant", "I'll edit the first file now. Adding the import for our new helpers and replacing the inline validation."),
        (19, "user", "Good. Move on to the next one."),
        (19, "assistant", "Updating the second file. This one also needs a middleware adjustment to pass the validated data through."),
        (20, "user", "And the third."),
        (
            20,
            "assistant",
            "The test file needs the most changes - I'm adding test cases for each of the new helper functions and removing the old inline validation tests.",
        ),
        # Fact insertion point 6 (turn 21-22)
        (21, "user", "Before we continue, we considered another approach earlier. What was it?"),
        (21, "assistant", "{FACT_5}. So we went with the current approach instead."),
        (22, "user", "Right. Let's continue with the implementation."),
        (
            22,
            "assistant",
            "Continuing with the refactoring. I'm now cleaning up the import statements and making sure all the type hints are correct.",
        ),
        (23, "user", "How's the code looking now?"),
        (
            23,
            "assistant",
            "Much cleaner. The routes file went from 120 lines to 65 lines, and the new helpers module is about 40 lines. All the validation logic is in one place now.",
        ),
        (24, "user", "Let's add proper error handling."),
        (
            24,
            "assistant",
            "I'll add try/except blocks around the database calls and return proper HTTP error responses with descriptive messages.",
        ),
        (25, "user", "Good. Now let's handle the edge cases."),
        (
            25,
            "assistant",
            "I see three edge cases we need to handle:\n1. Empty request body on POST\n2. Non-integer user ID in the URL path\n3. Database connection timeout",
        ),
        (26, "user", "Implement those."),
        (26, "assistant", "Done. Each edge case now returns an appropriate 4xx status code with a JSON error body."),
        (27, "user", "Let's also add input sanitization."),
        (
            27,
            "assistant",
            "I'll add HTML escaping for user-provided strings and length limits for text fields. This prevents both XSS and database field overflow.",
        ),
        (28, "user", "Run the test suite."),
        (
            28,
            "assistant",
            "Running pytest... 12 tests passed, 2 failed. The failures are in the old integration tests that expect the inline validation format.",
        ),
        (29, "user", "Fix those tests."),
        (29, "assistant", "Updated the integration tests to match the new validation helper interface. All 14 tests pass now."),
        (30, "user", "Great work. Let's move on to the next phase."),
        (30, "assistant", "Ready for the next phase. We've completed the refactoring and all tests pass. The codebase is in a clean state."),
        # -- COMPACTION TRIGGER at turn 31 --
        (31, "user", "Let's now add caching to improve performance."),
        (
            31,
            "assistant",
            "I'll add an in-memory LRU cache for the user lookup endpoint since it's the most frequently called. I'll use `functools.lru_cache` for simplicity.",
        ),
        (32, "user", "Implement the cache."),
        (
            32,
            "assistant",
            "Created the cache wrapper. It caches up to 1000 entries with a 5-minute TTL. I've also added a cache invalidation hook on user updates.",
        ),
        (33, "user", "How do we handle cache invalidation on deletes?"),
        (
            33,
            "assistant",
            "On DELETE /users/{id}, I'll call `cache.invalidate(user_id)` before returning the response. This ensures stale data is never served.",
        ),
        (34, "user", "Add monitoring for cache hit rates."),
        (
            34,
            "assistant",
            "Added a `/metrics` endpoint that reports cache hits, misses, and evictions. We can use this to tune the cache size.",
        ),
        (35, "user", "Run tests again with the cache enabled."),
        (
            35,
            "assistant",
            "All 16 tests pass (14 existing + 2 new cache tests). The cache correctly handles the happy path and invalidation scenarios.",
        ),
        (36, "user", "Let's also add rate limiting."),
        (36, "assistant", "I'll add a simple token bucket rate limiter in the middleware. Default: 100 requests per minute per IP."),
        (37, "user", "Implement it."),
        (
            37,
            "assistant",
            "Done. The rate limiter uses a dict of IP -> (tokens, last_refill_time). It returns 429 with a Retry-After header when the limit is exceeded.",
        ),
        (38, "user", "Test the rate limiter."),
        (38, "assistant", "Added 3 tests for rate limiting: normal usage, hitting the limit, and token refill after wait. All pass."),
        (39, "user", "Good. Any other improvements we should make?"),
        (
            39,
            "assistant",
            "I'd suggest adding structured logging with request IDs for observability. This will help with debugging in production.",
        ),
        (40, "user", "Add that too."),
        (
            40,
            "assistant",
            "Added a request ID middleware that generates a UUID per request, attaches it to the log context, and includes it in the response headers.",
        ),
    ]


def _build_template_debug_and_fix() -> list[tuple[int, str, str]]:
    """Template 2: debug a failing CI pipeline, fix issues, improve tooling."""
    return [
        (1, "user", "Our CI pipeline has been failing for 2 days. Help me debug it."),
        (1, "assistant", "Let me look at the project structure and CI configuration first."),
        (2, "user", "Here's the CI log showing the failure."),
        (2, "assistant", "I see the pipeline fails at the test stage. Let me examine the test configuration and recent changes."),
        (3, "user", "What files were changed recently?"),
        (
            3,
            "assistant",
            "According to git log, the last 5 commits changed:\n- `src/processor.py` (data pipeline refactor)\n- `src/models.py` (added new fields)\n- `tests/test_processor.py` (updated test data)\n- `setup.cfg` (bumped dependency version)",
        ),
        (4, "user", "Let's focus on the processor changes."),
        (
            4,
            "assistant",
            "The processor was refactored to use a streaming approach instead of loading everything into memory. The old `process_all()` was split into smaller functions.",
        ),
        (5, "user", "Show me the new function signatures."),
        (5, "assistant", "Let me read the processor module."),
        # Fact insertion point 1 (turn 6)
        (6, "user", "What new functions were added?"),
        (6, "assistant", "{FACT_0}. These replace the old monolithic `process_all()` function."),
        (7, "user", "Let's run the failing tests locally."),
        (7, "assistant", "Running `pytest tests/test_processor.py -v`..."),
        (8, "user", "What's the output?"),
        (8, "assistant", "4 tests pass, 2 tests fail. The failures are in `test_batch_processing` and `test_error_handling`."),
        (9, "user", "Show me the error for test_batch_processing."),
        (
            9,
            "assistant",
            "The test expects `process_all()` to return a list, but the refactored code now yields items via a generator. The test needs updating.",
        ),
        (10, "user", "And the second failure?"),
        # Fact insertion point 2 (turn 10-11)
        (10, "assistant", "{FACT_1}. The root cause is a missing null check in the new streaming code."),
        (11, "user", "Fix both issues."),
        (
            11,
            "assistant",
            "Fixed:\n1. Updated test to consume the generator with `list()`\n2. Added null check before subscript access\nAll 6 tests pass now.",
        ),
        (12, "user", "Good. But I want to understand the design choice better."),
        # Fact insertion point 3 (turn 13)
        (13, "user", "Why did we choose this architecture?"),
        (13, "assistant", "{FACT_2}. This lets us process datasets larger than available RAM."),
        (14, "user", "Were there other approaches considered?"),
        # Fact insertion point 4 (turn 14)
        (14, "assistant", "{FACT_5}. The streaming approach is simpler and sufficient for our scale."),
        (15, "user", "What constraints do we need to respect?"),
        # Fact insertion point 5 (turn 15)
        (15, "assistant", "{FACT_3}. This means we need to be careful with our streaming implementation."),
        (16, "user", "Which files are involved in this change?"),
        # Fact insertion point 6 (turn 16-17)
        (17, "user", "List them all."),
        (17, "assistant", "{FACT_4}. I've verified all of them compile and pass tests."),
        (18, "user", "Now let's improve the CI config."),
        (18, "assistant", "I'll add caching for pip dependencies and parallelize the test matrix."),
        (19, "user", "Also add a lint step."),
        (
            19,
            "assistant",
            "Added a ruff lint step that runs before tests. This catches style issues early without waiting for the full test suite.",
        ),
        (20, "user", "What about type checking?"),
        (
            20,
            "assistant",
            "I'll add a mypy step. The project doesn't have py.typed or stubs yet, so I'll start with `--ignore-missing-imports` and strict mode for our own code.",
        ),
        (21, "user", "Run the full pipeline locally."),
        (
            21,
            "assistant",
            "Running lint -> typecheck -> test:\n1. Ruff: 2 warnings (unused imports) - fixed\n2. Mypy: 1 error (missing return type) - fixed\n3. Pytest: all 8 tests pass",
        ),
        (22, "user", "Let's add coverage reporting."),
        (22, "assistant", "Added `pytest-cov` with a minimum coverage threshold of 80%. Current coverage is 73%."),
        (23, "user", "How can we get to 80%?"),
        (
            23,
            "assistant",
            "The uncovered code is mostly error handling paths. I'll add tests for:\n- Network timeout handling\n- Malformed input data\n- Database connection failures",
        ),
        (24, "user", "Add those tests."),
        (24, "assistant", "Added 5 new tests covering the error paths. Coverage is now 84%."),
        (25, "user", "Good. Let's also add a pre-commit hook."),
        (25, "assistant", "I'll set up pre-commit with ruff (lint + format) and mypy. This ensures code quality before it hits CI."),
        (26, "user", "Configure it."),
        (
            26,
            "assistant",
            "Created `.pre-commit-config.yaml` with hooks for ruff-check, ruff-format, and mypy. Running `pre-commit install` to set up the git hook.",
        ),
        (27, "user", "Test the pre-commit hook."),
        (27, "assistant", "Ran `pre-commit run --all-files`. All hooks pass. The hook adds about 3 seconds to each commit."),
        (28, "user", "Let's finalize the CI improvements."),
        (
            28,
            "assistant",
            "Final CI pipeline:\n1. Lint (ruff) - 10s\n2. Type check (mypy) - 15s\n3. Test (pytest with coverage) - 45s\n4. Build (wheel) - 5s\nTotal: ~75s, down from ~180s before parallelization.",
        ),
        (29, "user", "Commit everything."),
        (
            29,
            "assistant",
            "Committed all changes in 3 logical commits:\n1. Fix processor streaming bug and update tests\n2. Add CI lint and type check steps\n3. Add pre-commit hooks and coverage config",
        ),
        (30, "user", "Push and verify CI passes."),
        (30, "assistant", "Pushed to the PR branch. CI is running... all 4 stages passed in 78 seconds. The pipeline is green again."),
        # -- COMPACTION TRIGGER at turn 31 --
        (31, "user", "Now let's add a new feature to the processor."),
        (31, "assistant", "What feature would you like to add?"),
        (32, "user", "Add support for CSV output in addition to JSON."),
        (
            32,
            "assistant",
            "I'll add a `--format` flag to the CLI with `json` and `csv` options. The processor will use the appropriate serializer based on the flag.",
        ),
        (33, "user", "Implement it."),
        (
            33,
            "assistant",
            "Added the CSV serializer. It uses the stdlib `csv` module with `DictWriter`. Headers are auto-detected from the first record.",
        ),
        (34, "user", "What about nested data?"),
        (
            34,
            "assistant",
            'For nested dicts, I flatten the keys with dot notation: `{"user": {"name": "x"}}` becomes `user.name`. Arrays are joined with semicolons.',
        ),
        (35, "user", "Test the CSV output."),
        (
            35,
            "assistant",
            "Added 3 tests: simple flat data, nested data, and mixed types. All pass. The CSV output matches the expected format.",
        ),
        (36, "user", "Add documentation for the new format option."),
        (
            36,
            "assistant",
            "Updated the README with examples for both JSON and CSV output. Also added docstrings to the new serializer functions.",
        ),
        (37, "user", "Let's also add YAML as a format option."),
        (
            37,
            "assistant",
            "I'll add YAML support but make it optional - it requires the `pyyaml` package. If not installed, the CLI will show a helpful error message.",
        ),
        (38, "user", "Good approach. Implement it."),
        (
            38,
            "assistant",
            "Done. The YAML serializer is conditionally imported. The `--format yaml` flag works when pyyaml is installed, and shows 'pip install pyyaml' when it's not.",
        ),
        (39, "user", "Run the full test suite."),
        (39, "assistant", "All 16 tests pass. Coverage is 82%. The CI pipeline should be happy with this."),
        (40, "user", "Let's wrap up."),
        (
            40,
            "assistant",
            "Summary of today's work:\n- Fixed the CI pipeline failure (streaming bug)\n- Added lint, type check, and coverage to CI\n- Set up pre-commit hooks\n- Added CSV and YAML output formats",
        ),
    ]


def _build_template_new_feature() -> list[tuple[int, str, str]]:
    """Template 3: build a new feature from scratch with design decisions."""
    return [
        (1, "user", "I need to add a data export feature to the dashboard application."),
        (1, "assistant", "Let me understand the existing codebase first. I'll look at the project structure."),
        (2, "user", "Show me the current architecture."),
        (
            2,
            "assistant",
            "The dashboard is a Flask app with:\n- `app/` - main application\n- `app/views/` - route handlers\n- `app/models/` - SQLAlchemy models\n- `app/services/` - business logic\n- `app/templates/` - Jinja2 templates",
        ),
        (3, "user", "The export should support filtering and multiple formats."),
        (3, "assistant", "Got it. I'll design the export service with a strategy pattern for format handling. Let me outline the approach."),
        (4, "user", "What's the plan?"),
        (
            4,
            "assistant",
            "1. Create an ExportService in `app/services/`\n2. Define format handlers (CSV, Excel, PDF)\n3. Add export endpoints to the views\n4. Wire up the UI export button\n5. Add tests",
        ),
        (5, "user", "Let's start. What functions will we need?"),
        # Fact insertion point 1 (turn 6)
        (6, "user", "Define the core functions."),
        (6, "assistant", "{FACT_0}. These form the core of the export pipeline."),
        (7, "user", "Good. Now let's think about the database layer."),
        (7, "assistant", "The export queries can be expensive. We need to handle pagination and streaming for large datasets."),
        (8, "user", "How should we handle the database connection?"),
        (
            8,
            "assistant",
            "I'll use SQLAlchemy's `yield_per()` for server-side cursors. This lets us stream rows without loading everything into memory.",
        ),
        (9, "user", "What about the configuration?"),
        # Fact insertion point 2 (turn 10)
        (10, "user", "Should we use YAML or TOML for the export config?"),
        (10, "assistant", "{FACT_2}. This keeps our dependency list lean."),
        (11, "user", "Now implement the export service."),
        (11, "assistant", "Creating `app/services/export_service.py` with the three core functions and format-specific handlers."),
        (12, "user", "I'm getting an error running the service."),
        # Fact insertion point 3 (turn 12)
        (12, "assistant", "{FACT_1}. I need to add a null check for the response body before trying to extract fields."),
        (13, "user", "Fix it and continue."),
        (
            13,
            "assistant",
            "Fixed. The export service now gracefully handles empty API responses by returning an empty dataset with headers only.",
        ),
        (14, "user", "What about performance requirements?"),
        # Fact insertion point 4 (turn 15)
        (15, "user", "The product team has some constraints."),
        (15, "assistant", "{FACT_3}. I'll make sure our streaming approach respects these limits."),
        (16, "user", "Were there other approaches we considered?"),
        # Fact insertion point 5 (turn 16)
        (16, "assistant", "{FACT_5}. The current approach is more maintainable for our team size."),
        (17, "user", "Which files do we need to change?"),
        # Fact insertion point 6 (turn 17)
        (17, "assistant", "{FACT_4}. Let me work through them systematically."),
        (18, "user", "Start with the service layer."),
        (
            18,
            "assistant",
            "Writing the ExportService class with dependency injection for the format handlers. Each handler implements a `write(data, output_stream)` method.",
        ),
        (19, "user", "Add the CSV handler."),
        (
            19,
            "assistant",
            "CSV handler uses stdlib `csv.DictWriter`. It auto-detects headers from the first row and handles encoding (UTF-8 BOM for Excel compatibility).",
        ),
        (20, "user", "Now the Excel handler."),
        (
            20,
            "assistant",
            "The Excel handler will use openpyxl. Since it's an optional dependency, I'll add a clear error message if it's not installed.",
        ),
        (21, "user", "Add the route handler."),
        (
            21,
            "assistant",
            "Added `GET /api/export` with query params: `format` (csv/xlsx), `filters` (JSON), `columns` (comma-separated). Returns a streaming response with appropriate Content-Type and Content-Disposition headers.",
        ),
        (22, "user", "Test the endpoint."),
        (
            22,
            "assistant",
            "Created test fixtures with sample data. The CSV export works correctly. The Excel export generates a valid .xlsx file that opens in LibreOffice.",
        ),
        (23, "user", "Add error handling."),
        (
            23,
            "assistant",
            "Added handling for:\n- Invalid format parameter -> 400 with supported formats list\n- Empty result set -> 200 with headers-only file\n- Database timeout -> 504 with retry suggestion\n- File size limit exceeded -> 413 with row count limit",
        ),
        (24, "user", "What about authentication?"),
        (
            24,
            "assistant",
            "The export endpoint inherits the app's auth middleware. I've added an additional check for export permissions - only users with the 'data_export' role can access it.",
        ),
        (25, "user", "Add audit logging for exports."),
        (
            25,
            "assistant",
            "Added an audit log entry for each export: user_id, timestamp, format, row_count, filters applied. Stored in the `audit_log` table.",
        ),
        (26, "user", "Now let's add the UI button."),
        (
            26,
            "assistant",
            "Added an 'Export' dropdown button to the dashboard header. It shows format options (CSV, Excel) and uses the current filter state from the URL params.",
        ),
        (27, "user", "Test the full flow."),
        (
            27,
            "assistant",
            "End-to-end test: login -> apply filters -> click export -> verify file download. Works correctly in Chrome and Firefox.",
        ),
        (28, "user", "Add progress indication for large exports."),
        (
            28,
            "assistant",
            "Added a progress bar using Server-Sent Events. The export endpoint sends progress updates (rows processed / total) and the frontend renders a progress bar with cancel option.",
        ),
        (29, "user", "Run the full test suite."),
        (
            29,
            "assistant",
            "All 22 tests pass. Coverage for the export module is 91%. The only uncovered code is the openpyxl import error path.",
        ),
        (30, "user", "Clean up and prepare for review."),
        (30, "assistant", "Cleaned up: removed debug prints, added docstrings, ran ruff format. Ready for code review."),
        # -- COMPACTION TRIGGER at turn 31 --
        (31, "user", "Now let's add scheduled exports as a follow-up."),
        (31, "assistant", "Good idea. Users should be able to schedule recurring exports (daily/weekly) that get emailed to them."),
        (32, "user", "How would we implement the scheduler?"),
        (
            32,
            "assistant",
            "I'll use APScheduler for the cron-like scheduling. Each scheduled export is stored in the database with: user_id, schedule_cron, format, filters, email_to.",
        ),
        (33, "user", "Implement the scheduler."),
        (
            33,
            "assistant",
            "Created `app/services/scheduler.py` with `add_job()`, `remove_job()`, and `list_jobs()`. Jobs are persisted in SQLite via APScheduler's SQLAlchemyJobStore.",
        ),
        (34, "user", "Add the API endpoints for managing schedules."),
        (
            34,
            "assistant",
            "Added CRUD endpoints:\n- `POST /api/export/schedules` - create\n- `GET /api/export/schedules` - list\n- `DELETE /api/export/schedules/{id}` - delete\n- `PATCH /api/export/schedules/{id}` - update",
        ),
        (35, "user", "How do we send the emails?"),
        (
            35,
            "assistant",
            "I'll use `smtplib` from stdlib with TLS. The email contains the export file as an attachment. Configuration is in environment variables (SMTP_HOST, SMTP_PORT, etc.).",
        ),
        (36, "user", "Test the scheduled export flow."),
        (
            36,
            "assistant",
            "Added integration tests with a mock SMTP server. The scheduler correctly triggers at the cron time, generates the export, and sends the email with attachment.",
        ),
        (37, "user", "Add error notifications."),
        (
            37,
            "assistant",
            "If a scheduled export fails (DB error, SMTP error), the user gets an error notification email with the failure reason and a link to retry manually.",
        ),
        (38, "user", "Any edge cases?"),
        (
            38,
            "assistant",
            "Handled:\n- User deleted but schedule still active -> auto-disable\n- Export generates empty file -> skip email, log warning\n- Concurrent schedule executions -> mutex per schedule ID",
        ),
        (39, "user", "Run everything."),
        (39, "assistant", "All 31 tests pass. The scheduler tests use time mocking to avoid actual delays. CI pipeline passes in 82 seconds."),
        (40, "user", "Finalize."),
        (
            40,
            "assistant",
            "All changes committed and pushed. The export feature is complete with on-demand and scheduled exports, audit logging, and progress indication.",
        ),
    ]


TEMPLATES = [
    _build_template_explore_and_refactor,
    _build_template_debug_and_fix,
    _build_template_new_feature,
]


def _recall_turns() -> list[tuple[int, str, str]]:
    """Recall question turns (41-50).  Slots {RECALL_Q_N} are filled later."""
    turns = []
    for i in range(8):  # up to 8 recall questions
        t = 41 + i
        turns.append((t, "user", "{RECALL_Q_" + str(i) + "}"))
        turns.append((t, "assistant", "(model response to be captured at runtime)"))
    return turns


# ---------------------------------------------------------------------------
# Generator
# ---------------------------------------------------------------------------


def generate_conversation(
    conv_id: str,
    *,
    facts_per: int,
    rng: random.Random,
) -> dict:
    """Generate a single conversation with planted facts."""
    template_fn = rng.choice(TEMPLATES)
    template = template_fn()

    # Sample facts
    facts = rng.sample(FACT_POOL, min(facts_per, len(FACT_POOL)))

    # Build fact insertion mapping: FACT_0..FACT_N -> fact text
    fact_map: dict[str, str] = {}
    planted: list[dict] = []

    for idx, fact in enumerate(facts):
        key = f"FACT_{idx}"
        fact_map[f"{{{key}}}"] = fact["fact"]

    # Build conversation messages, substituting facts
    conversation: list[dict] = []
    for turn_num, role, content in template:
        resolved = content
        for slot, replacement in fact_map.items():
            if slot in resolved:
                resolved = resolved.replace(slot, replacement)
                # Record which fact was planted at which turn
                slot_idx = int(slot.strip("{}").split("_")[1])
                if slot_idx < len(facts):
                    planted.append(
                        {
                            "turn": turn_num,
                            "category": facts[slot_idx]["category"],
                            "fact": facts[slot_idx]["fact"],
                            "recall_question": facts[slot_idx]["recall_question"],
                            "expected_answer_contains": facts[slot_idx]["expected_answer_contains"],
                        }
                    )
        conversation.append({"turn": turn_num, "role": role, "content": resolved})

    # De-duplicate planted facts (a fact may appear in multiple template slots
    # that reference the same FACT_N)
    seen_facts: set[str] = set()
    unique_planted: list[dict] = []
    for p in planted:
        if p["fact"] not in seen_facts:
            seen_facts.add(p["fact"])
            unique_planted.append(p)

    # Add recall question turns
    for i, fact in enumerate(unique_planted):
        t = 41 + i
        conversation.append({"turn": t, "role": "user", "content": fact["recall_question"]})
        conversation.append({"turn": t, "role": "assistant", "content": "(to be captured at runtime)"})

    return {
        "id": conv_id,
        "conversation": conversation,
        "planted_facts": unique_planted,
        "compaction_trigger_turn": 31,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate compaction-recall conversation fixtures")
    parser.add_argument("--conversations", type=int, default=10, help="Number of conversations to generate (default: 10)")
    parser.add_argument("--facts-per", type=int, default=6, help="Planted facts per conversation (default: 6)")
    parser.add_argument("-o", "--output", type=Path, default=Path("fixtures"), help="Output directory (default: fixtures/)")
    parser.add_argument("--seed", type=int, default=None, help="Random seed for reproducibility")
    args = parser.parse_args()

    rng = random.Random(args.seed)
    args.output.mkdir(parents=True, exist_ok=True)

    conversations: list[dict] = []
    for i in range(args.conversations):
        conv_id = f"recall-{i + 1:03d}"
        conv = generate_conversation(conv_id, facts_per=args.facts_per, rng=rng)
        conversations.append(conv)
        print(f"Generated {conv_id}: {len(conv['planted_facts'])} facts planted", file=sys.stderr)

    # Write individual files
    for conv in conversations:
        path = args.output / f"{conv['id']}.json"
        path.write_text(json.dumps(conv, indent=2, ensure_ascii=False) + "\n")

    # Write combined file
    combined = args.output / "all.json"
    combined.write_text(json.dumps(conversations, indent=2, ensure_ascii=False) + "\n")

    total_facts = sum(len(c["planted_facts"]) for c in conversations)
    print(f"\nGenerated {len(conversations)} conversations with {total_facts} total planted facts", file=sys.stderr)
    print(f"Output: {args.output}/", file=sys.stderr)


if __name__ == "__main__":
    main()
