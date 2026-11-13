-- The balances and statistics join every obligation (or share) to its entry only to read the
-- entry's status: a narrow index answers that without touching the wide entry rows.
create index ledger_entries_status_by_id on ledger_entries (id) include (status);
