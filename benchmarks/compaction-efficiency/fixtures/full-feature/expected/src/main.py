"""Entry point for the data pipeline."""

from src.cache import LRUCache
from src.config import Config
from src.processor import process


def main() -> None:
    config = Config()
    cache = LRUCache(max_size=config.cache_size)

    record_ids = [1, 2, 3, 1, 2, 4, 5, 1]
    results = process(record_ids, cache=cache)
    for r in results:
        print(r)

    print(f"Cache stats: {cache.stats()}")


if __name__ == "__main__":
    main()
