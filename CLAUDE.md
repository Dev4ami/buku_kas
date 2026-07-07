# keuangan-bot

Bot Telegram pribadi untuk mencatat keuangan harian. Milik satu user (owner-only), deploy di home server via Coolify.

## Arsitektur

- **Rust + teloxide 0.13** — bot framework, long polling (bukan webhook, biar gak perlu expose port)
- **sqlx 0.8 + Postgres** — storage. Migrations otomatis jalan saat startup (`sqlx::migrate!`)
- **Axum 0.8** — web dashboard read-only, jalan sebagai tokio task berdampingan bot
- **Timezone**: semua laporan dihitung dalam WIB (Asia/Jakarta), disimpan sebagai TIMESTAMPTZ (UTC)

## Struktur

```
src/
├── main.rs      # entry, dispatcher, handlers (command, text, callback)
├── parser.rs    # parse input santai: "15k soto", "1.5jt kos", "+5jt gaji"
├── db.rs        # semua query sqlx
├── report.rs    # laporan /hariini & /bulanini
└── web.rs       # dashboard read-only (Axum), spawn dari main
migrations/
└── 0001_init.sql
static/
├── index.html    # UI dashboard, di-embed via include_str!
└── chart.umd.js  # Chart.js bundled, di-embed via include_str!
```

## Web dashboard

- Endpoint: `/` (HTML), `/login` GET+POST, `/logout`, `/chart.js`, `/api/summary?month=YYYY-MM`
- Auth: password + cookie sesi. Kalau `DASHBOARD_PASSWORD` di-set, akses `/` dan `/api/summary` diarahkan ke `/login`. Kalau kosong, dashboard terbuka tanpa auth (dev only).
- Cookie sesi: `dashboard_session`, isinya `HMAC-SHA256(SESSION_SECRET, "dashboard-authorized-v1")` base64url. HttpOnly, SameSite=Lax, Max-Age 30 hari.
- `SESSION_SECRET` harus stabil (jangan diganti antar deploy — semua sesi login lama bakal invalid).
- Bind address: `BIND_ADDR` env (default `0.0.0.0:8765`)
- **Read-only**: cuma SELECT, nol operasi tulis — biar aman dijadiin snapshot bulanan tanpa risiko korupsi data

## Flow utama

1. User kirim `15k soto` → `parser::parse` ekstrak amount + note
2. Insert ke `transactions` (category_id masih NULL)
3. Bot balas inline keyboard kategori (callback data: `cat:<tx_id>:<cat_id>`)
4. User tap → `set_tx_category`, pesan di-edit jadi konfirmasi
5. Tombol `🗑 batal` (callback `del:<tx_id>`) → hapus transaksi

## Aturan penting

- **`amount` selalu BIGINT Rupiah utuh. JANGAN PERNAH float.**
- **Owner-only**: semua handler cek `OWNER_ID`. Pesan dari orang lain di-ignore diam-diam (jangan balas apapun)
- Income ditandai prefix `+` di input, `tx_type = 'income'`, tanpa kategori
- Kategori sengaja cuma 7 — jangan tambah tanpa diminta

## Env vars

Lihat `.env.example`. Wajib: `TELOXIDE_TOKEN`, `OWNER_ID`, `DATABASE_URL`. Opsional dashboard: `DASHBOARD_PASSWORD`, `SESSION_SECRET`, `BIND_ADDR`.

## Development

```bash
cargo test              # unit test parser
cargo run               # butuh .env terisi
```

Catatan: `sqlx::migrate!` butuh folder `migrations/` ada saat compile. Query pakai runtime-checked (`query_as` biasa), bukan compile-time macros, jadi gak butuh DATABASE_URL saat build.

## Deploy (Coolify)

- Build pakai Dockerfile (multi-stage, image akhir ~20MB)
- Postgres: pakai instance shared yang sudah ada, bikin database + user baru khusus `keuangan`. Jangan expose port 5432 ke publik
- Kalau build OOM di server (i5-6500, RAM terbatas): build image di mesin lain / GitHub Actions, push ke registry, deploy prebuilt image

## Roadmap (belum dikerjakan)

- [ ] Laporan bulanan otomatis via cron (kirim tiap tanggal 1)
- [ ] Chart pengeluaran pakai `plotters`, kirim sebagai gambar
- [ ] Auto-guess kategori dari keyword note ("soto" → makan)
- [ ] `/hapus <id>` untuk hapus transaksi lama
- [ ] Tabel `budgets` + alert kalau mendekati limit
