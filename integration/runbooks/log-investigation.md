# 本地故障检索

1. 从请求保存quoteId、chainKey、quoteViewId、SnapshotRef、configRelease及economicPolicyHash。不要保存Authorization或secret值。
2. 在dex-router JSON日志按quote_id定位quote_completed，核对canonical_epoch、online_generation、graph_version和策略hash。
3. 认证读取/internal/quotes/{id}/evidence和/internal/chains/{chain}/status；过期证据仍显式显示expired和snapshot_valid。
4. 在owner解析器核对同一chain/generation/journal_seq的baseline_committed；parser状态ready不等于router图ready。
5. STATE_TOO_OLD表示在线年龄超预算；历史gap独立存在不阻断当前scope。不能通过改时间或忽略旧writer使其ready。
6. 查看/metrics中的log_dropped_total/write_errors_total。队列满时调试日志可能被拒绝，必须结合drop指标；当前尚无持久spool/集中采集/审计日志，不能宣称日志零丢失。
7. 源读取不确定或依赖缺失时暂停范围并恢复可信当前基线；生产Kafka/PG/reorg/多区演练尚未接通，不能将本地恢复手册当生产恢复已验收。
