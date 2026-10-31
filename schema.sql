-- Original GoTalk schema. Opt-in for NEW databases; no migration on production startup.
CREATE TABLE IF NOT EXISTS messages (
    id SERIAL PRIMARY KEY,
    content TEXT,
    sender_pod TEXT,
    sender_nick TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS users (
    nickname TEXT PRIMARY KEY,
    color_code TEXT
);
