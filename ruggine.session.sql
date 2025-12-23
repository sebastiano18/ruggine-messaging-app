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
    last_read_sequence INTEGER NOT NULL DEFAULT 0, -- ⭐ AGGIUNGI QUESTA RIGA
    PRIMARY KEY (conversation_id, user_id)
);


CREATE INDEX idx_participants_last_read ON participants(user_id, conversation_id, last_read_sequence);

CREATE TABLE messages
(
    id              TEXT PRIMARY KEY, -- UUID
    conversation_id TEXT    NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    author_id       TEXT    NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    content         TEXT    NOT NULL,
    created_at      INTEGER NOT NULL,
    sequence_num    INTEGER -- Nuovo campo per sequence tracking
);

CREATE TABLE invites
(
    id              TEXT PRIMARY KEY, -- UUID
    conversation_id TEXT        NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    token           TEXT UNIQUE NOT NULL,
    expires_at      INTEGER     NOT NULL,
    used            INTEGER     NOT NULL DEFAULT 0
);

-- Indexes for messages
CREATE INDEX idx_msgs_conv_ts ON messages (conversation_id, created_at);
CREATE INDEX idx_msgs_conv_seq ON messages (conversation_id, sequence_num);
CREATE UNIQUE INDEX idx_msgs_conv_seq_unique ON messages (conversation_id, sequence_num) WHERE sequence_num IS NOT NULL;

-- User events table (per eventi a livello utente)
CREATE TABLE user_events
(
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         TEXT    NOT NULL,
    sequence_num    INTEGER NOT NULL,
    event_type      TEXT    NOT NULL,
    event_data      TEXT    NOT NULL,
    conversation_id TEXT    NULL,
    created_at      INTEGER NOT NULL,
    delivered       BOOLEAN DEFAULT FALSE,

    UNIQUE (user_id, sequence_num)
);

-- Indexes for user events performance
CREATE INDEX idx_user_events_seq ON user_events (user_id, sequence_num);
CREATE INDEX idx_user_events_undelivered ON user_events (user_id, delivered) WHERE delivered = FALSE;
CREATE INDEX idx_user_events_cleanup ON user_events (created_at) WHERE delivered = TRUE;

-- Table to track current sequence per user (per eventi utente)
CREATE TABLE user_sequences
(
    user_id            TEXT PRIMARY KEY,
    current_sequence   INTEGER NOT NULL DEFAULT 0,
    last_ping_sequence INTEGER          DEFAULT 0,
    last_ping_at       INTEGER          DEFAULT 0
);

-- NUOVO: Table to track message sequences per conversation
CREATE TABLE message_sequences
(
    conversation_id  TEXT PRIMARY KEY,
    current_sequence INTEGER NOT NULL DEFAULT 0,
    last_updated     INTEGER NOT NULL
);