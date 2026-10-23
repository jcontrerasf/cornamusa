CREATE TABLE accounts (
    id            TEXT PRIMARY KEY,
    provider      TEXT NOT NULL,
    email         TEXT NOT NULL,
    display_name  TEXT NOT NULL,
    username      TEXT NOT NULL,
    imap_json     TEXT NOT NULL,
    smtp_json     TEXT NOT NULL,
    color         TEXT NOT NULL,
    position      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE folders (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    path          TEXT NOT NULL,
    name          TEXT NOT NULL,
    role          TEXT NOT NULL,
    uid_validity  INTEGER,
    present       INTEGER NOT NULL DEFAULT 1,
    UNIQUE (account_id, path)
);

CREATE TABLE messages (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    folder_id        INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    uid              INTEGER NOT NULL,
    message_id       TEXT,
    thread_id        TEXT NOT NULL,
    subject          TEXT NOT NULL,
    from_json        TEXT,
    to_json          TEXT NOT NULL,
    cc_json          TEXT NOT NULL,
    date             INTEGER NOT NULL,
    preview          TEXT NOT NULL DEFAULT '',
    seen             INTEGER NOT NULL DEFAULT 0,
    flagged          INTEGER NOT NULL DEFAULT 0,
    answered         INTEGER NOT NULL DEFAULT 0,
    has_attachments  INTEGER NOT NULL DEFAULT 0,
    in_reply_to      TEXT,
    refs             TEXT NOT NULL DEFAULT '[]',
    size             INTEGER NOT NULL DEFAULT 0,
    raw              BLOB,
    UNIQUE (folder_id, uid)
);
CREATE INDEX messages_folder_date ON messages(folder_id, date DESC);
CREATE INDEX messages_thread ON messages(account_id, thread_id);
CREATE INDEX messages_msgid ON messages(account_id, message_id);

CREATE VIRTUAL TABLE messages_fts USING fts5(
    subject, people, body,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TABLE signatures (
    id          TEXT PRIMARY KEY,
    account_id  TEXT REFERENCES accounts(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    is_html     INTEGER NOT NULL,
    body        TEXT NOT NULL,
    is_default  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

CREATE TABLE trusted_senders (
    email  TEXT PRIMARY KEY
);
