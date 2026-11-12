-- Periodic background jobs (housekeeping). Each row is claimed with FOR UPDATE SKIP LOCKED, so
-- with several processes every run happens exactly once. Data-driven jobs (poll deadlines, pact
-- nudges) need no row here: they claim their own due rows the same way.
create table scheduled_jobs (
    name        text        primary key,
    next_run_at timestamptz not null default now(),
    last_run_at timestamptz
);
