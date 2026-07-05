-- Skema awal: categories + transactions
-- Prinsip: amount pakai BIGINT (Rupiah utuh), JANGAN float.

CREATE TABLE IF NOT EXISTS categories (
    id      SERIAL PRIMARY KEY,
    name    TEXT NOT NULL UNIQUE,
    emoji   TEXT NOT NULL DEFAULT '💸'
);

CREATE TABLE IF NOT EXISTS transactions (
    id          BIGSERIAL PRIMARY KEY,
    amount      BIGINT NOT NULL CHECK (amount > 0),
    category_id INT REFERENCES categories(id),
    note        TEXT,
    tx_type     TEXT NOT NULL DEFAULT 'expense' CHECK (tx_type IN ('expense', 'income')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_tx_created_at ON transactions (created_at);
CREATE INDEX IF NOT EXISTS idx_tx_category ON transactions (category_id);

-- Kategori default: sengaja sedikit biar gak overwhelm
INSERT INTO categories (name, emoji) VALUES
    ('makan',     '🍜'),
    ('transport', '🛵'),
    ('jajan',     '🧋'),
    ('tagihan',   '🧾'),
    ('belanja',   '🛒'),
    ('hiburan',   '🎮'),
    ('lainnya',   '💸')
ON CONFLICT (name) DO NOTHING;
