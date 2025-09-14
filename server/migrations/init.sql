PRAGMA foreign_keys = ON;

CREATE TABLE users
(
    id         TEXT PRIMARY KEY, -- UUID
    username   TEXT UNIQUE NOT NULL,
    pass_hash  TEXT        NOT NULL,
    created_at INTEGER     NOT NULL
);

CREATE TABLE conversations
(
    id         TEXT PRIMARY KEY, -- UUID
    kind       TEXT    NOT NULL CHECK (kind IN ('dm', 'group')),
    title      TEXT,
    owner_id   TEXT    NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL
);

CREATE TABLE participants
(
    conversation_id TEXT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    user_id         TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role            TEXT NOT NULL DEFAULT 'member',
    last_read_msg   TEXT, -- può referenziare messages.id se serve
    PRIMARY KEY (conversation_id, user_id)
);

CREATE TABLE messages
(
    id              TEXT PRIMARY KEY, -- UUID
    conversation_id TEXT    NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    author_id       TEXT    NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    content         TEXT    NOT NULL,
    created_at      INTEGER NOT NULL
);

CREATE TABLE invites
(
    id              TEXT PRIMARY KEY, -- UUID
    conversation_id TEXT        NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    token           TEXT UNIQUE NOT NULL,
    expires_at      INTEGER     NOT NULL,
    used            INTEGER     NOT NULL DEFAULT 0
);

CREATE INDEX idx_msgs_conv_ts ON messages (conversation_id, created_at);

