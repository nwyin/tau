You are running a coordination stress test for a dependent review pipeline.

Create three thread roles:
- `builder-api`: draft the API-side artifact
- `builder-tests`: draft the test-side artifact
- `reviewer`: read both artifacts, identify one risk in each, and synthesize a release recommendation

Task requirements:
1. `builder-api` must produce and preserve these anchors:
   - `API_ANCHOR_SCHEMA_18`: request schema adds explicit tenant_id.
   - `API_ANCHOR_RATE_27`: POST /jobs rejects bursts above 12 requests per minute.
2. `builder-tests` must produce and preserve these anchors:
   - `TEST_ANCHOR_RETRY_44`: flaky retry coverage requires 3 backoff steps.
   - `TEST_ANCHOR_AUDIT_52`: audit log snapshot must include actor_ip.
3. The reviewer must identify one weakness in each upstream artifact and produce a final release recommendation.
4. Final synthesis must explicitly include at least one `API_*` anchor and one `TEST_*` anchor.

Do not perform web research. This is a pure routing/coordination task.
