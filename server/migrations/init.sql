PRAGMA foreign_keys = ON;


CREATE TABLE users (
                       id INTEGER PRIMARY KEY,
                       username TEXT UNIQUE NOT NULL,
                       pass_hash TEXT NOT NULL,
                       created_at INTEGER NOT NULL
);


CREATE TABLE groups (
                        id INTEGER PRIMARY KEY,
                        name TEXT NOT NULL,
                        owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                        created_at INTEGER NOT NULL
);


CREATE TABLE group_members (
                               user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                               group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
                               role TEXT NOT NULL DEFAULT 'member',
                               PRIMARY KEY(user_id, group_id)
);


CREATE TABLE invites (
                         id INTEGER PRIMARY KEY,
                         group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
                         token TEXT UNIQUE NOT NULL,
                         expires_at INTEGER NOT NULL,
                         used INTEGER NOT NULL DEFAULT 0
);


-- Conversazioni DM o di gruppo (room)
CREATE TABLE conversations (
                               id INTEGER PRIMARY KEY,
                               kind TEXT NOT NULL CHECK(kind IN ('dm','group')),
                               title TEXT,
                               created_at INTEGER NOT NULL
);


CREATE TABLE participants (
                              conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                              user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                              role TEXT NOT NULL DEFAULT 'member',
                              last_read_msg INTEGER,
                              PRIMARY KEY(conversation_id, user_id)
);


CREATE TABLE messages (
                          id INTEGER PRIMARY KEY,
                          conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                          author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                          body TEXT NOT NULL,
                          created_at INTEGER NOT NULL
);


CREATE INDEX idx_msgs_conv_ts ON messages(conversation_id, created_at);