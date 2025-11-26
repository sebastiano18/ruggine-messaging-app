PRAGMA foreign_keys = ON;

-- ============================================================================
-- SCHEMA
-- ============================================================================

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
    last_read_msg   TEXT,
    last_read_sequence INTEGER NOT NULL DEFAULT 0,
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
    sequence_num    INTEGER
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

-- User events table
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

CREATE INDEX idx_user_events_seq ON user_events (user_id, sequence_num);
CREATE INDEX idx_user_events_undelivered ON user_events (user_id, delivered) WHERE delivered = FALSE;
CREATE INDEX idx_user_events_cleanup ON user_events (created_at) WHERE delivered = TRUE;

CREATE TABLE user_sequences
(
    user_id            TEXT PRIMARY KEY,
    current_sequence   INTEGER NOT NULL DEFAULT 0,
    last_ping_sequence INTEGER          DEFAULT 0,
    last_ping_at       INTEGER          DEFAULT 0
);

CREATE TABLE message_sequences
(
    conversation_id  TEXT PRIMARY KEY,
    current_sequence INTEGER NOT NULL DEFAULT 0,
    last_updated     INTEGER NOT NULL
);

-- ============================================================================
-- SEED DATA con UUID VALIDI
-- ============================================================================

-- 👥 Utenti con password "password"
INSERT INTO users (id, username, pass_hash, created_at) VALUES
                                                            ('cb2b5063-aec6-46f2-a289-fa654ec1b049', 'alice', '$argon2id$v=19$m=65536,t=3,p=4$eksJ0Nj6JjARShFv6MMsbw$MRYhXaz3fV4es39v3M8IcpO4d1fZXy92KM76Ce8FY/I', 1704067200),
                                                            ('ae6d82c6-a7c3-47c8-b16b-11f27b82232c', 'bob', '$argon2id$v=19$m=65536,t=3,p=4$eksJ0Nj6JjARShFv6MMsbw$MRYhXaz3fV4es39v3M8IcpO4d1fZXy92KM76Ce8FY/I', 1704067200),
                                                            ('9b60b6f7-3d43-43de-b477-b9683f3f3c6e', 'charlie', '$argon2id$v=19$m=65536,t=3,p=4$eksJ0Nj6JjARShFv6MMsbw$MRYhXaz3fV4es39v3M8IcpO4d1fZXy92KM76Ce8FY/I', 1704067200),
                                                            ('b6fbd855-de10-4074-b3d0-04d714140780', 'diana', '$argon2id$v=19$m=65536,t=3,p=4$eksJ0Nj6JjARShFv6MMsbw$MRYhXaz3fV4es39v3M8IcpO4d1fZXy92KM76Ce8FY/I', 1704067200);

-- 💬 Conversazioni DM
INSERT INTO conversations (id, kind, title, owner_id, created_at) VALUES
                                                                      ('957b8b8f-6052-41d5-ad27-ca43f4cda3f8', 'dm', NULL, 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 1704067300),
                                                                      ('74bbf474-69b9-48af-a88f-b2758022f908', 'dm', NULL, '9b60b6f7-3d43-43de-b477-b9683f3f3c6e', 1704067400);

-- 👥 Conversazioni di Gruppo
INSERT INTO conversations (id, kind, title, owner_id, created_at) VALUES
                                                                      ('7fd142a0-61ed-4096-a8cb-50c1c5b532d9', 'group', 'Team Alpha', 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 1704067500),
                                                                      ('152ee9f8-db96-48bc-9c26-9abc6f63cab6', 'group', 'Random Chat', 'b6fbd855-de10-4074-b3d0-04d714140780', 1704067600);

-- 🔗 Partecipanti - DM Alice ↔ Bob
INSERT INTO participants (conversation_id, user_id, role, last_read_sequence) VALUES
                                                                                  ('957b8b8f-6052-41d5-ad27-ca43f4cda3f8', 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 'member', 3),
                                                                                  ('957b8b8f-6052-41d5-ad27-ca43f4cda3f8', 'ae6d82c6-a7c3-47c8-b16b-11f27b82232c', 'member', 3);

-- 🔗 Partecipanti - DM Charlie ↔ Diana
INSERT INTO participants (conversation_id, user_id, role, last_read_sequence) VALUES
                                                                                  ('74bbf474-69b9-48af-a88f-b2758022f908', '9b60b6f7-3d43-43de-b477-b9683f3f3c6e', 'member', 2),
                                                                                  ('74bbf474-69b9-48af-a88f-b2758022f908', 'b6fbd855-de10-4074-b3d0-04d714140780', 'member', 2);

-- 🔗 Partecipanti - Gruppo "Team Alpha"
INSERT INTO participants (conversation_id, user_id, role, last_read_sequence) VALUES
                                                                                  ('7fd142a0-61ed-4096-a8cb-50c1c5b532d9', 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 'owner', 5),
                                                                                  ('7fd142a0-61ed-4096-a8cb-50c1c5b532d9', 'ae6d82c6-a7c3-47c8-b16b-11f27b82232c', 'member', 4),
                                                                                  ('7fd142a0-61ed-4096-a8cb-50c1c5b532d9', '9b60b6f7-3d43-43de-b477-b9683f3f3c6e', 'member', 5);

-- 🔗 Partecipanti - Gruppo "Random Chat"
INSERT INTO participants (conversation_id, user_id, role, last_read_sequence) VALUES
                                                                                  ('152ee9f8-db96-48bc-9c26-9abc6f63cab6', 'b6fbd855-de10-4074-b3d0-04d714140780', 'owner', 6),
                                                                                  ('152ee9f8-db96-48bc-9c26-9abc6f63cab6', 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 'member', 5),
                                                                                  ('152ee9f8-db96-48bc-9c26-9abc6f63cab6', 'ae6d82c6-a7c3-47c8-b16b-11f27b82232c', 'member', 6),
                                                                                  ('152ee9f8-db96-48bc-9c26-9abc6f63cab6', '9b60b6f7-3d43-43de-b477-b9683f3f3c6e', 'member', 4);

-- 💬 Messaggi - DM Alice ↔ Bob
INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES
                                                                                             ('4ac0883f-7166-4628-ba46-ad34df612271', '957b8b8f-6052-41d5-ad27-ca43f4cda3f8', 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 'Hey Bob! How are you?', 1704067350, 1),
                                                                                             ('dee55638-2172-4509-a12c-2e0834e75c9b', '957b8b8f-6052-41d5-ad27-ca43f4cda3f8', 'ae6d82c6-a7c3-47c8-b16b-11f27b82232c', 'Hi Alice! I''m doing great, thanks! Working on a new Rust project.', 1704067380, 2),
                                                                                             ('fb3d8f15-4f2c-43d2-87b1-07ae178ac6fc', '957b8b8f-6052-41d5-ad27-ca43f4cda3f8', 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 'Awesome! Is it using egui by any chance?', 1704067420, 3);

-- 💬 Messaggi - DM Charlie ↔ Diana
INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES
                                                                                             ('edd1dcb4-b9c4-4c96-a439-2e6edc4a1530', '74bbf474-69b9-48af-a88f-b2758022f908', '9b60b6f7-3d43-43de-b477-b9683f3f3c6e', 'Diana, did you see the new migration system?', 1704067450, 1),
                                                                                             ('aae786fa-8d25-4055-9cf5-dd8c10b578bf', '74bbf474-69b9-48af-a88f-b2758022f908', 'b6fbd855-de10-4074-b3d0-04d714140780', 'Yes! It''s working perfectly now', 1704067480, 2);

-- 💬 Messaggi - Gruppo "Team Alpha"
INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES
                                                                                             ('d7eb300b-0f30-460d-a7e5-1cffb51e9d50', '7fd142a0-61ed-4096-a8cb-50c1c5b532d9', 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 'Welcome to Team Alpha everyone!', 1704067550, 1),
                                                                                             ('d67002db-3e8e-4976-86d8-c9461aa2e112', '7fd142a0-61ed-4096-a8cb-50c1c5b532d9', 'ae6d82c6-a7c3-47c8-b16b-11f27b82232c', 'Thanks Alice! Excited to be here.', 1704067580, 2),
                                                                                             ('f7e4579b-610d-4b7e-88fd-8d73bb6a9470', '7fd142a0-61ed-4096-a8cb-50c1c5b532d9', '9b60b6f7-3d43-43de-b477-b9683f3f3c6e', 'Let''s build something amazing!', 1704067610, 3),
                                                                                             ('e25e7d32-477d-4a83-a1a7-3097d7b915c7', '7fd142a0-61ed-4096-a8cb-50c1c5b532d9', 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 'That''s the spirit! First task: review the WebSocket implementation.', 1704067640, 4),
                                                                                             ('e8928f8a-4fbc-4c01-b17f-7fa7c5b902f3', '7fd142a0-61ed-4096-a8cb-50c1c5b532d9', 'ae6d82c6-a7c3-47c8-b16b-11f27b82232c', 'On it!', 1704067670, 5);

-- 💬 Messaggi - Gruppo "Random Chat"
INSERT INTO messages (id, conversation_id, author_id, content, created_at, sequence_num) VALUES
                                                                                             ('e124aaad-4681-48a6-907f-81dc1273f173', '152ee9f8-db96-48bc-9c26-9abc6f63cab6', 'b6fbd855-de10-4074-b3d0-04d714140780', 'Hey everyone! This is our random chat group.', 1704067650, 1),
                                                                                             ('b01d7181-34ac-4beb-b135-0216f6daeeba', '152ee9f8-db96-48bc-9c26-9abc6f63cab6', 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 'Cool! What should we talk about?', 1704067680, 2),
                                                                                             ('78d0250e-9551-429d-a5d3-aa21bf8521ae', '152ee9f8-db96-48bc-9c26-9abc6f63cab6', '9b60b6f7-3d43-43de-b477-b9683f3f3c6e', 'How about favorite programming languages?', 1704067710, 3),
                                                                                             ('e8b9c71d-735c-40cc-a783-edf90bab843e', '152ee9f8-db96-48bc-9c26-9abc6f63cab6', 'ae6d82c6-a7c3-47c8-b16b-11f27b82232c', 'Rust all the way!', 1704067740, 4),
                                                                                             ('970f4021-7ef3-4cc3-89c1-7e2cb9e03515', '152ee9f8-db96-48bc-9c26-9abc6f63cab6', 'cb2b5063-aec6-46f2-a289-fa654ec1b049', 'Agreed! The type system is incredible.', 1704067770, 5),
                                                                                             ('28d8d23b-80f7-4753-af90-f83db4d135e8', '152ee9f8-db96-48bc-9c26-9abc6f63cab6', 'ae6d82c6-a7c3-47c8-b16b-11f27b82232c', 'And the compiler messages are so helpful!', 1704067800, 6);

-- 📊 Inizializza message_sequences
INSERT INTO message_sequences (conversation_id, current_sequence, last_updated) VALUES
                                                                                    ('957b8b8f-6052-41d5-ad27-ca43f4cda3f8', 3, 1704067420),
                                                                                    ('74bbf474-69b9-48af-a88f-b2758022f908', 2, 1704067480),
                                                                                    ('7fd142a0-61ed-4096-a8cb-50c1c5b532d9', 5, 1704067670),
                                                                                    ('152ee9f8-db96-48bc-9c26-9abc6f63cab6', 6, 1704067800);

-- 📊 Inizializza user_sequences
INSERT INTO user_sequences (user_id, current_sequence, last_ping_sequence, last_ping_at) VALUES
                                                                                             ('cb2b5063-aec6-46f2-a289-fa654ec1b049', 0, 0, 1704067200),
                                                                                             ('ae6d82c6-a7c3-47c8-b16b-11f27b82232c', 0, 0, 1704067200),
                                                                                             ('9b60b6f7-3d43-43de-b477-b9683f3f3c6e', 0, 0, 1704067200),
                                                                                             ('b6fbd855-de10-4074-b3d0-04d714140780', 0, 0, 1704067200);