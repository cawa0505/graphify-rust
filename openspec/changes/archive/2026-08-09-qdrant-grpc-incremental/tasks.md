## 1. Config Updates

- [x] 1.1 Support `grpc` Boolean field under `long_term.qdrant` config in `graphify-llm/src/config.rs`
- [x] 1.2 Include custom `indexing_threshold` (default 20000 KB) option under `long_term.qdrant` to store the target optimal value

## 2. Qdrant HNSW Tuning

> **[已否決 2026-08-09]** HNSW tuning 不實作：gRPC/HNSW 流式增量複雜度高、ROI 低，Qdrant 寫入依 record_id 冪等，增量機制已定案為 SQL-driven 篩選 + REST/SDK batch upsert（P2/P4 落地）。

- [ ] 2.1 Add HNSW optimization toggle requests in `graphify-llm/src/memory.rs`
- [ ] 2.2 Disable indexing before starting node upload batches by setting `indexing_threshold` to `0`
- [ ] 2.3 Restore indexing to configured threshold post-upload (using non-blocking `wait=false` parameters)

## 3. Optional gRPC Transport Mode

> **[已否決 2026-08-09]** gRPC transport 不實作：理由同上（見 §2 註記）。傳輸統一走 qdrant-client SDK / REST。

- [ ] 3.1 Setup conditional gRPC transport connection using `qdrant-client` if configured port `6334` is enabled
- [ ] 3.2 Implement fallback connection to REST `6333` if gRPC endpoint fails or is unreachable

## 4. SHA256-Based Incremental Sync

> **[已實作]** SHA256 增量快照已在 graphify-cli/src/snapshot.rs（snapshot 存 graphify-out/.graphify-snapshot.json）。

- [ ] 4.1 Create a lightweight snapshot registry under `graphify-out/` or `.graphify_cache` tracking file paths and SHA256 content hashes
- [ ] 4.2 Skip parsing, embedding generation, and indexing for files that match their active SHA256 hashes
- [ ] 4.3 Automatically delete obsolete vectors in Qdrant via payload filter on modification before uploading fresh points

## 5. Metadata Indexing & Payload Filtering

> **[已實作]** payload keyword index 已在 graphify-memory/src/memory.rs:474 create_field_index（source_file/kind/language）。

- [ ] 5.1 Issue direct REST/gRPC index-creation calls on metadata fields (`source_file`, `kind`, `language`) during collection setup
- [ ] 5.2 Implement Payload-prefiltering filters during vector search queries to restrict scope and avoid full scans
