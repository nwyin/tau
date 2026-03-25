window.BENCHMARK_DATA = {
  "lastUpdate": 1774425897873,
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
          "id": "263db748821e2dedbad7e13e4cdd20d5dc0181a9",
          "message": "fix: accept static-pie in musl linkage check for Ubuntu 24.04",
          "timestamp": "2026-03-21T10:26:35+08:00",
          "tree_id": "91c496f99fb446e6c3b5beb01086ebbb1073bce6",
          "url": "https://github.com/nwyin/tau/commit/263db748821e2dedbad7e13e4cdd20d5dc0181a9"
        },
        "date": 1774060259702,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2167,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3764,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13113,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21309,
            "range": "± 112",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 95273,
            "range": "± 556",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3724,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17928,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 36932,
            "range": "± 331",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12666,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 74712,
            "range": "± 3124",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 144777,
            "range": "± 515",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 16921,
            "range": "± 61",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92062,
            "range": "± 239",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 182282,
            "range": "± 405",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2877,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 39342,
            "range": "± 126",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 362052,
            "range": "± 1473",
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
          "id": "5a97a3a08cc5e5104ce6f12a7a634da9b676ab8c",
          "message": "fix: set dummy API key in serve tests for CI\n\nThe serve integration tests exercise the JSON-RPC protocol, not LLM\ncalls. But agent construction requires a valid API key, which doesn't\nexist in CI. Set a dummy OPENAI_API_KEY env var so the process starts.",
          "timestamp": "2026-03-21T13:42:34+08:00",
          "tree_id": "b5aeeec9690af226a8a1e613f8dfcff8706e6e39",
          "url": "https://github.com/nwyin/tau/commit/5a97a3a08cc5e5104ce6f12a7a634da9b676ab8c"
        },
        "date": 1774072059833,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 1956,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 4164,
            "range": "± 58",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13971,
            "range": "± 123",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 25344,
            "range": "± 135",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 108469,
            "range": "± 3630",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3231,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 15976,
            "range": "± 147",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 31958,
            "range": "± 342",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 11807,
            "range": "± 430",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 66760,
            "range": "± 420",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 138407,
            "range": "± 410",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 15648,
            "range": "± 99",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 83953,
            "range": "± 4306",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 166861,
            "range": "± 2773",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2621,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38457,
            "range": "± 159",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 359614,
            "range": "± 2037",
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
          "id": "39c411115758a85f1b01e63bd331fa4c33f9d326",
          "message": "fix: update CI workflow for tau binary rename\n\nThe binary was renamed from coding-agent to tau but CI still referenced\nthe old name in the musl build, linkage check, and release asset steps.",
          "timestamp": "2026-03-21T13:50:15+08:00",
          "tree_id": "d4ce0be25a68b4353383dd01824e696b122366e9",
          "url": "https://github.com/nwyin/tau/commit/39c411115758a85f1b01e63bd331fa4c33f9d326"
        },
        "date": 1774072490693,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2177,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3779,
            "range": "± 462",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 12325,
            "range": "± 98",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 23305,
            "range": "± 233",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 97819,
            "range": "± 786",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3719,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 18214,
            "range": "± 185",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 36282,
            "range": "± 1234",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12864,
            "range": "± 167",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73890,
            "range": "± 352",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 145845,
            "range": "± 733",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17196,
            "range": "± 83",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92693,
            "range": "± 1080",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 183564,
            "range": "± 577",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2868,
            "range": "± 25",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38458,
            "range": "± 349",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 361368,
            "range": "± 1077",
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
          "id": "aab79209c82d1f482436ac2723338786d6fd5b77",
          "message": "docs: add context window management survey and design",
          "timestamp": "2026-03-21T14:25:46+08:00",
          "tree_id": "06b903eb53628e718be60fd07028b6e4d00c4719",
          "url": "https://github.com/nwyin/tau/commit/aab79209c82d1f482436ac2723338786d6fd5b77"
        },
        "date": 1774074633177,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2221,
            "range": "± 88",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3879,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13158,
            "range": "± 370",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 22931,
            "range": "± 442",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 98033,
            "range": "± 1167",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3651,
            "range": "± 101",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 18050,
            "range": "± 252",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 35650,
            "range": "± 143",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12606,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73002,
            "range": "± 197",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 144268,
            "range": "± 1225",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17017,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 91898,
            "range": "± 490",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 181753,
            "range": "± 2819",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2958,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38638,
            "range": "± 142",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 363923,
            "range": "± 6329",
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
          "id": "b0dfb1fca0eb20629029c0adb38f3210f1598cb9",
          "message": "docs: expand context management survey with all local harnesses",
          "timestamp": "2026-03-21T14:31:05+08:00",
          "tree_id": "02781b05e40fcc94596c104a60fc95b11ca7e92a",
          "url": "https://github.com/nwyin/tau/commit/b0dfb1fca0eb20629029c0adb38f3210f1598cb9"
        },
        "date": 1774074959349,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2170,
            "range": "± 104",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3745,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13092,
            "range": "± 71",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21324,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 94215,
            "range": "± 776",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3621,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 18483,
            "range": "± 183",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 36989,
            "range": "± 294",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12911,
            "range": "± 56",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 75482,
            "range": "± 311",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 149810,
            "range": "± 602",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17266,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 93200,
            "range": "± 853",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 184091,
            "range": "± 1530",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2867,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38656,
            "range": "± 181",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 362816,
            "range": "± 1840",
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
          "id": "f43bc8d37ec3d2b14d2d4b28f53456c3aaea608f",
          "message": "docs: add underexplored harness design dimensions to feature comparison",
          "timestamp": "2026-03-21T14:46:27+08:00",
          "tree_id": "71c283986e2f98c130d378ddbe87d8a4da4a5f14",
          "url": "https://github.com/nwyin/tau/commit/f43bc8d37ec3d2b14d2d4b28f53456c3aaea608f"
        },
        "date": 1774075882830,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2294,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3844,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13274,
            "range": "± 61",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 23746,
            "range": "± 62",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 113038,
            "range": "± 700",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3655,
            "range": "± 51",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 18244,
            "range": "± 534",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 36185,
            "range": "± 204",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12800,
            "range": "± 221",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73218,
            "range": "± 1220",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 145418,
            "range": "± 416",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17312,
            "range": "± 51",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92645,
            "range": "± 945",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 182738,
            "range": "± 909",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2872,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38801,
            "range": "± 240",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 359977,
            "range": "± 970",
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
          "id": "98bfa7fe845e8a702d43a0953b9dff746271bdad",
          "message": "docs: document harness vs orchestrator separation rationale",
          "timestamp": "2026-03-21T15:00:17+08:00",
          "tree_id": "b5fb2384de9036920f9e43502ce7c31e091dade4",
          "url": "https://github.com/nwyin/tau/commit/98bfa7fe845e8a702d43a0953b9dff746271bdad"
        },
        "date": 1774076695263,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2235,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3816,
            "range": "± 53",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13294,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21657,
            "range": "± 78",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 98229,
            "range": "± 880",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3699,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17998,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 36325,
            "range": "± 109",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 13014,
            "range": "± 79",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73917,
            "range": "± 2581",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 145654,
            "range": "± 324",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17167,
            "range": "± 90",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 94090,
            "range": "± 227",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 184292,
            "range": "± 594",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2916,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38979,
            "range": "± 106",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 363502,
            "range": "± 707",
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
          "id": "efa7c654a6b1d0ab3211b70c4423504887a2fbe2",
          "message": "feat: add OpenAI Chat Completions provider for OpenRouter support\n\nAdd a third API provider backend (openai-chat) that speaks the standard\n/v1/chat/completions SSE protocol. This covers OpenRouter (200+ models),\nplus any OpenAI-compatible endpoint (Groq, Together, Ollama, etc.).\n\nNew files:\n- openai_chat.rs: provider struct, key resolution, streaming HTTP\n- openai_chat_shared.rs: message conversion, tool format, SSE state machine\n- provider-consolidation.md: architecture plan and migration guide\n\n10 OpenRouter models added to catalog (Gemini 3.1, Grok 4.20, Qwen 3.5,\nDevstral, Mistral Small, DeepSeek, Kimi). Agent builder updated with\nkey resolution for openrouter/groq/together/deepseek providers.",
          "timestamp": "2026-03-21T22:53:35+08:00",
          "tree_id": "64b902f400c57afa5419ced72f7a34f185e77ee3",
          "url": "https://github.com/nwyin/tau/commit/efa7c654a6b1d0ab3211b70c4423504887a2fbe2"
        },
        "date": 1774133668206,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2132,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3763,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13203,
            "range": "± 147",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21459,
            "range": "± 274",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 95404,
            "range": "± 269",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3392,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17289,
            "range": "± 73",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34704,
            "range": "± 306",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12901,
            "range": "± 78",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73801,
            "range": "± 386",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 145508,
            "range": "± 692",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17060,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 91634,
            "range": "± 1742",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 182342,
            "range": "± 1155",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2874,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38914,
            "range": "± 217",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 364117,
            "range": "± 1130",
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
          "id": "b4d011dfe2b5ec1a37917303f8bb49f5c2ca6c6f",
          "message": "feat: add `tau models` subcommand to list available models\n\nShows all registered models grouped by provider with pricing, context\nwindow, and API backend. Supports --provider/-p filter for narrowing\nto a specific provider (e.g. `tau models -p openrouter`).",
          "timestamp": "2026-03-22T07:03:57+08:00",
          "tree_id": "76351add7845bfca164823b42a6dec8898ac337c",
          "url": "https://github.com/nwyin/tau/commit/b4d011dfe2b5ec1a37917303f8bb49f5c2ca6c6f"
        },
        "date": 1774134724450,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2152,
            "range": "± 67",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3780,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13110,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 23277,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 95391,
            "range": "± 698",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3433,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17319,
            "range": "± 155",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34873,
            "range": "± 147",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12958,
            "range": "± 64",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73994,
            "range": "± 338",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 150512,
            "range": "± 581",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17040,
            "range": "± 67",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92572,
            "range": "± 832",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 183801,
            "range": "± 467",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2857,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38707,
            "range": "± 163",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 361636,
            "range": "± 1163",
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
          "id": "b445cf5f7186f3e8703fad5dff6731c49ca028a7",
          "message": "fix benchmark adapters for tau binary rename, add tracking plan\n\nHarbor adapter:\n- Binary name coding-agent → tau throughout\n- Forward OPENROUTER_API_KEY to containers\n- Report version from TAU_VERSION env var\n- Use _BINARY_DEST constant for run command\n\nTerminal-bench adapter:\n- Binary path default /usr/local/bin/coding-agent → /usr/local/bin/tau\n- Process detection pgrep -f coding-agent → tau\n\nBoth install scripts updated to reference tau binary name.\n\nAdded docs/benchmark-tracking.md with design for release-gated\nterminal-bench evaluation with model × version matrix.",
          "timestamp": "2026-03-22T07:32:31+08:00",
          "tree_id": "bdf5fe3ad927fa6d8b76e9f56b123d0a4928a74f",
          "url": "https://github.com/nwyin/tau/commit/b445cf5f7186f3e8703fad5dff6731c49ca028a7"
        },
        "date": 1774136338741,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2145,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3705,
            "range": "± 227",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13019,
            "range": "± 150",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21486,
            "range": "± 416",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 97257,
            "range": "± 491",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3377,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17131,
            "range": "± 58",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34244,
            "range": "± 616",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12988,
            "range": "± 118",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73439,
            "range": "± 471",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 146237,
            "range": "± 506",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 16771,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92036,
            "range": "± 243",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 182290,
            "range": "± 652",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2894,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38678,
            "range": "± 212",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 364070,
            "range": "± 1889",
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
          "id": "e6c497e9af3b55f457adec2b22664f35fcd76105",
          "message": "update hive",
          "timestamp": "2026-03-22T22:16:06+08:00",
          "tree_id": "ffa2205a9c29d8c4f60ccaac5533fd9453cc5b30",
          "url": "https://github.com/nwyin/tau/commit/e6c497e9af3b55f457adec2b22664f35fcd76105"
        },
        "date": 1774194253695,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2118,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 4661,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13177,
            "range": "± 105",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 23262,
            "range": "± 107",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 108414,
            "range": "± 1532",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3399,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17295,
            "range": "± 4911",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34496,
            "range": "± 121",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 13009,
            "range": "± 77",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 74093,
            "range": "± 331",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 147722,
            "range": "± 534",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17231,
            "range": "± 113",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 94624,
            "range": "± 3189",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 182071,
            "range": "± 700",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2856,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38723,
            "range": "± 711",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 359659,
            "range": "± 1688",
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
          "id": "9487260af29a4eda2a1434698847dc59a8a455e3",
          "message": "add benchmark specs and shared template for 9 microbenchmarks\n\nMove microbenchmarks.md from docs/ to benchmarks/, slim it to an\nindex table. Add TEMPLATE.md with shared patterns (fixture formats,\nsession management, A/B test pattern, reporting). Add SPEC.md for\neach benchmark with detailed fixtures, variants, metrics, and\ndecision criteria. Add shared/ infrastructure design (TauSession,\nBenchConfig, TaskResult, Reporter, Verifier).",
          "timestamp": "2026-03-23T12:05:30+08:00",
          "tree_id": "08d002214b5e01c21572798fa4b0d06822ce9e3b",
          "url": "https://github.com/nwyin/tau/commit/9487260af29a4eda2a1434698847dc59a8a455e3"
        },
        "date": 1774239607965,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2138,
            "range": "± 98",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3710,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13149,
            "range": "± 76",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 23210,
            "range": "± 119",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 96475,
            "range": "± 630",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3431,
            "range": "± 67",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17325,
            "range": "± 60",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34547,
            "range": "± 105",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12938,
            "range": "± 69",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 74011,
            "range": "± 679",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 150587,
            "range": "± 1086",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17015,
            "range": "± 118",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 91661,
            "range": "± 326",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 182440,
            "range": "± 524",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2880,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38537,
            "range": "± 125",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 362969,
            "range": "± 4906",
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
          "id": "4d7cb5389326fa57da53367760120d55f4d8d39a",
          "message": "add result storage layer: local JSON + R2 remote sync via rclone\n\nshared/store.py handles saving benchmark results locally and optionally\npushing to Cloudflare R2 for persistence across machines and CI. Results\nare queryable with DuckDB on both local files and S3. Gracefully no-ops\nwhen rclone or TAU_BENCH_REMOTE aren't configured.",
          "timestamp": "2026-03-23T12:45:37+08:00",
          "tree_id": "197ac1dc7780a009e1023d2258fe974fedf02472",
          "url": "https://github.com/nwyin/tau/commit/4d7cb5389326fa57da53367760120d55f4d8d39a"
        },
        "date": 1774241451194,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2133,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3734,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 12886,
            "range": "± 128",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 23199,
            "range": "± 583",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 94757,
            "range": "± 822",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3420,
            "range": "± 69",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17803,
            "range": "± 279",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34645,
            "range": "± 2550",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 13234,
            "range": "± 98",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 75482,
            "range": "± 294",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 153164,
            "range": "± 465",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17355,
            "range": "± 60",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 94300,
            "range": "± 354",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 185483,
            "range": "± 604",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2852,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38952,
            "range": "± 178",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 360781,
            "range": "± 1461",
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
          "id": "9e37ac8f3740eeba90f9f68a5acbec624855ce14",
          "message": "add commit miner for real-world benchmark fixtures\n\nshared/miner.py mines git commit history to generate benchmark fixtures.\nTwo strategies: 'edit' (single-file changes for fuzzy-e2e) and 'refactor'\n(multi-file type propagation for post-edit-diagnostics). Tested against\nhive and irradiate repos. Update microbenchmarks.md with fixture source\ntracking (synthetic vs hand-crafted vs mined).",
          "timestamp": "2026-03-23T13:38:32+08:00",
          "tree_id": "337076f7ec3401d7b3a2873dfc67f646bf192198",
          "url": "https://github.com/nwyin/tau/commit/9e37ac8f3740eeba90f9f68a5acbec624855ce14"
        },
        "date": 1774244602921,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2119,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3777,
            "range": "± 103",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 12983,
            "range": "± 141",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21229,
            "range": "± 248",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 95039,
            "range": "± 637",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3391,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17312,
            "range": "± 40",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34208,
            "range": "± 147",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12969,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73357,
            "range": "± 302",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 146288,
            "range": "± 1282",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17069,
            "range": "± 944",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92168,
            "range": "± 1068",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 181798,
            "range": "± 687",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2891,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38632,
            "range": "± 126",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 364298,
            "range": "± 1651",
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
          "id": "4fa66ccbeb8325b44b8d798f180b5214d439c097",
          "message": "consolidate fuzzy-match and fuzzy-false-positive into one benchmark\n\nMerge adversarial corpus generation into generate_corpus.py (accuracy\nvs adversarial subcommands). Unify runner to auto-detect corpus type\nand print the appropriate scorecard. Delete fuzzy-false-positive/\ndirectory. 9 benchmarks -> 8.",
          "timestamp": "2026-03-23T14:14:41+08:00",
          "tree_id": "b4f26e619352028b6e7356bcf15b5ef0c9aa5eac",
          "url": "https://github.com/nwyin/tau/commit/4fa66ccbeb8325b44b8d798f180b5214d439c097"
        },
        "date": 1774247081468,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2209,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3853,
            "range": "± 57",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13193,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 22773,
            "range": "± 166",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 103214,
            "range": "± 1676",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3389,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17330,
            "range": "± 205",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 33748,
            "range": "± 733",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12886,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73455,
            "range": "± 376",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 147242,
            "range": "± 304",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 16899,
            "range": "± 61",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 91930,
            "range": "± 412",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 182224,
            "range": "± 420",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2875,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38728,
            "range": "± 431",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 363458,
            "range": "± 1284",
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
          "id": "84bcf01af5e70cede56bc7a9fb98e675696423c7",
          "message": "remove fuzzy-e2e benchmark\n\nRedundant with edit-bench which already measures end-to-end edit success.\nThe offline fuzzy-match benchmark proved the strategy; no need for a\nseparate $60-100 model-in-loop benchmark. 8 benchmarks -> 7, total\nestimated cost drops from $85-150 to $43-68.",
          "timestamp": "2026-03-23T14:21:09+08:00",
          "tree_id": "9be1280a7376873ba5535ac46ceea517cfc3c586",
          "url": "https://github.com/nwyin/tau/commit/84bcf01af5e70cede56bc7a9fb98e675696423c7"
        },
        "date": 1774247177409,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2192,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3766,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13096,
            "range": "± 167",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21407,
            "range": "± 107",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 95909,
            "range": "± 428",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3414,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17134,
            "range": "± 215",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34039,
            "range": "± 204",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12871,
            "range": "± 91",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 73356,
            "range": "± 351",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 148616,
            "range": "± 8622",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 16865,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 91724,
            "range": "± 396",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 181857,
            "range": "± 9941",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2969,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38531,
            "range": "± 131",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 361607,
            "range": "± 2764",
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
          "id": "6cf5c1cf320d6838b8457098cece8b311e1034b2",
          "message": "feat: implement mechanical context compaction\n\nAdd agent::context module with three-tier compaction to prevent context\nwindow overflow without any LLM calls:\n\n  Tier 1 — truncate tool outputs exceeding 50KB or 2000 lines (40%+40%)\n  Tier 2 — mask old turns, replacing tool-result content with omission\n            placeholders while keeping tool-call names visible\n  Fallback — aggressively truncate remaining large tool results (20%+20%)\n\nBudget: (context_window × 0.75) − max_tokens − 2000 token system-prompt\nestimate. Zero overhead when messages fit within budget (early return).\n\nWire the transform_context hook in agent_builder so compaction fires\nautomatically before every LLM call in the coding-agent binary.\n\n12 tests in agent/tests/context_test.rs covering all five invariants:\nINV-1 identity, INV-2 first-user-preserved, INV-3 turn integrity,\nINV-4 budget satisfied, INV-5 tool name in placeholder.",
          "timestamp": "2026-03-23T16:58:26+08:00",
          "tree_id": "27046160b5facf3d17789aed9733d1c0591b8e59",
          "url": "https://github.com/nwyin/tau/commit/6cf5c1cf320d6838b8457098cece8b311e1034b2"
        },
        "date": 1774256771084,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2178,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3763,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13094,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 23285,
            "range": "± 123",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 96912,
            "range": "± 398",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3388,
            "range": "± 128",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17383,
            "range": "± 57",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34484,
            "range": "± 117",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12938,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 75361,
            "range": "± 190",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 150737,
            "range": "± 641",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17232,
            "range": "± 73",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92611,
            "range": "± 416",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 183028,
            "range": "± 480",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2867,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38008,
            "range": "± 214",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 363589,
            "range": "± 2363",
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
          "id": "395e9f089595bce8fc089b7fc11b97dacf45e38a",
          "message": "fix: format web_search tool and tests (rustfmt)",
          "timestamp": "2026-03-23T17:35:43+08:00",
          "tree_id": "03951554545cee87f1f529a547aa77f42e3e53b6",
          "url": "https://github.com/nwyin/tau/commit/395e9f089595bce8fc089b7fc11b97dacf45e38a"
        },
        "date": 1774261959881,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2161,
            "range": "± 94",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 4771,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13100,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21208,
            "range": "± 74",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 95635,
            "range": "± 602",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3542,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17123,
            "range": "± 67",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34691,
            "range": "± 135",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12773,
            "range": "± 110",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 74057,
            "range": "± 448",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 147086,
            "range": "± 805",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17087,
            "range": "± 170",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 91751,
            "range": "± 1337",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 182861,
            "range": "± 629",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2888,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38833,
            "range": "± 142",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 363683,
            "range": "± 1515",
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
          "id": "2013727d7f8274c3cac20910cbc3ff1129663156",
          "message": "chore: update default model to gpt-5.4-mini",
          "timestamp": "2026-03-24T12:11:39+08:00",
          "tree_id": "5b3502f3105e2fa3cc8252df3e9be994b8249250",
          "url": "https://github.com/nwyin/tau/commit/2013727d7f8274c3cac20910cbc3ff1129663156"
        },
        "date": 1774326201785,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2106,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 4778,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 12403,
            "range": "± 59",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 23198,
            "range": "± 273",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 96875,
            "range": "± 472",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3407,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17218,
            "range": "± 136",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34217,
            "range": "± 97",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 12953,
            "range": "± 272",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 76155,
            "range": "± 973",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 147854,
            "range": "± 1995",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17146,
            "range": "± 44",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 93507,
            "range": "± 373",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 183937,
            "range": "± 411",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2886,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38552,
            "range": "± 179",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 360561,
            "range": "± 1495",
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
          "id": "2c17d44a80df4d6eefa64644364b1456f24285ed",
          "message": "feat: verbose tool call logging with per-tool details",
          "timestamp": "2026-03-24T12:26:02+08:00",
          "tree_id": "1d6524e4a51262d5d4daf117a358a2b1bcecfc08",
          "url": "https://github.com/nwyin/tau/commit/2c17d44a80df4d6eefa64644364b1456f24285ed"
        },
        "date": 1774326647469,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2133,
            "range": "± 108",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3651,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 12969,
            "range": "± 44",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 21576,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 95542,
            "range": "± 357",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3424,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17209,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34524,
            "range": "± 168",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 13063,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 77406,
            "range": "± 374",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 149562,
            "range": "± 605",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17169,
            "range": "± 511",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 94165,
            "range": "± 516",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 184453,
            "range": "± 351",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2862,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38706,
            "range": "± 101",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 361397,
            "range": "± 1837",
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
          "id": "37944affe6c46bbf96094fde1bf4a22023ab60eb",
          "message": "feat: add subagent tool for delegating tasks to isolated subprocess\n\nSpawns `tau -p \"<task>\" --yolo --no-skills --no-session` as a child\nprocess with configurable model and timeout (default 5min). Sub-agent\ngets fresh context with default tools minus subagent itself to prevent\nunbounded recursion. Output captured from stdout and returned as text.",
          "timestamp": "2026-03-24T14:51:56+08:00",
          "tree_id": "404882df4110a627a676539c8bf6d12ab9037d2d",
          "url": "https://github.com/nwyin/tau/commit/37944affe6c46bbf96094fde1bf4a22023ab60eb"
        },
        "date": 1774339286437,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2111,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3793,
            "range": "± 23",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 12844,
            "range": "± 60",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 22936,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 94067,
            "range": "± 1666",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3401,
            "range": "± 77",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17413,
            "range": "± 119",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34420,
            "range": "± 232",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 13064,
            "range": "± 301",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 74430,
            "range": "± 731",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 148168,
            "range": "± 391",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17227,
            "range": "± 53",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92581,
            "range": "± 283",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 184199,
            "range": "± 11888",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2884,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38682,
            "range": "± 225",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 362894,
            "range": "± 3628",
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
          "id": "2dc71da2cab95c136c06cc65f84041916218d67f",
          "message": "feat: execute tool calls concurrently instead of sequentially\n\nWhen the model emits multiple tool calls in one turn (e.g. 5 greps),\nthey now run in parallel via tokio::spawn + join_all. Events are\nemitted in order: all ToolExecutionStart first, then ToolExecutionEnd\nas results arrive. Steering is checked once after the batch completes\ninstead of between each tool.",
          "timestamp": "2026-03-25T15:58:57+08:00",
          "tree_id": "f0b06d0c7ec34a866c6f01a27e91e2c59c35d237",
          "url": "https://github.com/nwyin/tau/commit/2dc71da2cab95c136c06cc65f84041916218d67f"
        },
        "date": 1774425897110,
        "tool": "cargo",
        "benches": [
          {
            "name": "agent_construction/new_agent",
            "value": 2164,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/10",
            "value": 3841,
            "range": "± 101",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/50",
            "value": 13543,
            "range": "± 88",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/100",
            "value": 22285,
            "range": "± 92",
            "unit": "ns/iter"
          },
          {
            "name": "agent_construction/replace_messages/500",
            "value": 100247,
            "range": "± 592",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/10",
            "value": 3436,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/50",
            "value": 17193,
            "range": "± 163",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/serialize/100",
            "value": 34499,
            "range": "± 428",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/10",
            "value": 13259,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/50",
            "value": 74292,
            "range": "± 222",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/deserialize/100",
            "value": 152051,
            "range": "± 6030",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/10",
            "value": 17286,
            "range": "± 113",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/50",
            "value": 92518,
            "range": "± 564",
            "unit": "ns/iter"
          },
          {
            "name": "message_serde/roundtrip/100",
            "value": 184141,
            "range": "± 542",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/10",
            "value": 2845,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/100",
            "value": 38825,
            "range": "± 246",
            "unit": "ns/iter"
          },
          {
            "name": "sse_parsing/events/1000",
            "value": 360988,
            "range": "± 1057",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}