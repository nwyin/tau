window.BENCHMARK_DATA = {
  "lastUpdate": 1773915253613,
  "repoUrl": "https://github.com/nwyin/tau",
  "entries": {
    "tau benchmarks": [
      {
        "commit": {
          "author": {
            "email": "tommynguyen0512@gmail.com",
            "name": "Tommy Bui Nguyen",
            "username": "nwyin"
          },
          "committer": {
            "email": "tommynguyen0512@gmail.com",
            "name": "Tommy Bui Nguyen",
            "username": "nwyin"
          },
          "distinct": true,
          "id": "c1c6f9f7de321fb85d91888ba81e8ea04fbe7438",
          "message": "add docs",
          "timestamp": "2026-03-19T18:08:36+08:00",
          "tree_id": "4bc2fdd4bf2cf39b604c25281d0cc953ef2488ef",
          "url": "https://github.com/nwyin/tau/commit/c1c6f9f7de321fb85d91888ba81e8ea04fbe7438"
        },
        "date": 1773915252846,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2124,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3828,
            "range": "± 78",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13035,
            "range": "± 92",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21292,
            "range": "± 82",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 96418,
            "range": "± 305",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3441,
            "range": "± 57",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17363,
            "range": "± 58",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34706,
            "range": "± 110",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12739,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73201,
            "range": "± 426",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 146676,
            "range": "± 2053",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17053,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 91978,
            "range": "± 301",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 181959,
            "range": "± 2793",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2719,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 37476,
            "range": "± 608",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 347972,
            "range": "± 11114",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}