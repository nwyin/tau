window.BENCHMARK_DATA = {
  "lastUpdate": 1774056680311,
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
      },
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
          "id": "a72eafa376214b2a8128cb226681a5060e44d741",
          "message": "chore: ignore Python __pycache__ and .pyc files\n\nPython compilation artifacts were showing as untracked in git status\nafter the terminal-bench adapter was added. Add standard Python entries\nto .gitignore to keep the working tree clean.",
          "timestamp": "2026-03-19T19:32:08+08:00",
          "tree_id": "66f8703fc0b66cbe062b990108db227284c3890b",
          "url": "https://github.com/nwyin/tau/commit/a72eafa376214b2a8128cb226681a5060e44d741"
        },
        "date": 1773920641264,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2170,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 4799,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13184,
            "range": "± 142",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 23222,
            "range": "± 59",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 94590,
            "range": "± 1061",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3678,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 18551,
            "range": "± 281",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 36299,
            "range": "± 142",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12829,
            "range": "± 123",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73210,
            "range": "± 497",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 145702,
            "range": "± 3441",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 16983,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92613,
            "range": "± 2389",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 182914,
            "range": "± 2596",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2733,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 36702,
            "range": "± 365",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 344725,
            "range": "± 1142",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "c3c2c85d63975c3b496aeec18b8a5ff3c0b1436b",
          "message": "fix: split concatenated gitignore patterns onto separate lines",
          "timestamp": "2026-03-20T16:08:42+08:00",
          "tree_id": "32796b6907c7f342a4fdd7a5018b805d0210fefb",
          "url": "https://github.com/nwyin/tau/commit/c3c2c85d63975c3b496aeec18b8a5ff3c0b1436b"
        },
        "date": 1773994460560,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 1952,
            "range": "± 25",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 4784,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13705,
            "range": "± 316",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 23895,
            "range": "± 461",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 105382,
            "range": "± 604",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3239,
            "range": "± 114",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 16145,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 31523,
            "range": "± 122",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 11699,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 66141,
            "range": "± 319",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 132080,
            "range": "± 752",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 15361,
            "range": "± 301",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 83920,
            "range": "± 3257",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 165264,
            "range": "± 496",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2626,
            "range": "± 98",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38894,
            "range": "± 149",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 360249,
            "range": "± 1336",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "e5cae196fdaee9f05445501bb890c8dd82f33acd",
          "message": "doc",
          "timestamp": "2026-03-20T19:51:43+08:00",
          "tree_id": "311b04cc7312c5db1d888f708c6aa6cff8511ee5",
          "url": "https://github.com/nwyin/tau/commit/e5cae196fdaee9f05445501bb890c8dd82f33acd"
        },
        "date": 1774007783519,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2139,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3663,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13070,
            "range": "± 67",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21274,
            "range": "± 251",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 95653,
            "range": "± 1514",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3676,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 18314,
            "range": "± 338",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 36598,
            "range": "± 145",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12871,
            "range": "± 525",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 72633,
            "range": "± 372",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 144750,
            "range": "± 719",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17028,
            "range": "± 143",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92047,
            "range": "± 436",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 182565,
            "range": "± 736",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2914,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38518,
            "range": "± 235",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 363190,
            "range": "± 1197",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "c7bf815a1e664b7cd9c5f5b3e600bdcce52fd355",
          "message": "fix: skip pycfg tests when binary not available in CI",
          "timestamp": "2026-03-21T09:26:15+08:00",
          "tree_id": "096622adb48b49d8a800d403459a44f18ad5acd0",
          "url": "https://github.com/nwyin/tau/commit/c7bf815a1e664b7cd9c5f5b3e600bdcce52fd355"
        },
        "date": 1774056679905,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2175,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3769,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 12996,
            "range": "± 86",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21088,
            "range": "± 120",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 94807,
            "range": "± 1553",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3726,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 18219,
            "range": "± 397",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 36906,
            "range": "± 94",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 13128,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 74785,
            "range": "± 212",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 147827,
            "range": "± 753",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17353,
            "range": "± 108",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 93399,
            "range": "± 514",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 185722,
            "range": "± 921",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2869,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38632,
            "range": "± 128",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 361547,
            "range": "± 4604",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}