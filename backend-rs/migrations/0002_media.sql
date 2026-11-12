-- Stored images. A media key (`<prefix>/<uuid>`) names a directory under MEDIA_ROOT holding a
-- lossless `master.webp` and lossy `sm.webp` / `md.webp` variants.

-- Images uploaded but not yet used by anything. Consuming one moves its files into place in the
-- same transaction; unconsumed ones expire after 30 minutes and are swept.
create table uploads (
    id         uuid        primary key,
    owner_id   bigint      not null references users on delete cascade,
    purpose    text        not null check (purpose in ('avatar', 'photo')),
    key        text        not null unique,
    width      integer     not null check (width > 0),
    height     integer     not null check (height > 0),
    created_at timestamptz not null default now(),
    expires_at timestamptz not null
);
create index uploads_expiry on uploads (expires_at);

-- Files to delete. Written in the same transaction that stops referencing them, so a rollback
-- never loses a file that is still referenced; a sweeper deletes them shortly after commit.
create table media_trash (
    key    text        primary key,
    due_at timestamptz not null default now()
);
create index media_trash_due on media_trash (due_at);
