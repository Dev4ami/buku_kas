-- Tambah kategori "rokok" biar gak campur sama "jajan"
INSERT INTO categories (name, emoji) VALUES
    ('rokok', '🚬')
ON CONFLICT (name) DO NOTHING;
