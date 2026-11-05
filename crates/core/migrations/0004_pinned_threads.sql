-- Conversations pinned to the top of the list. Local only: IMAP has no equivalent.
CREATE TABLE pinned_threads (
    account_id  TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id   TEXT NOT NULL,
    pinned_at   INTEGER NOT NULL,
    PRIMARY KEY (account_id, thread_id)
);
