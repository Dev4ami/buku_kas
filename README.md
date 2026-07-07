# 💰 Buku Kas

Bot Telegram pribadi buat catat pengeluaran tanpa ribet, plus dashboard web read-only. Ketik `15k soto`, tap kategori, selesai.

## Fitur

- ⚡ Input super cepat: `15k soto`, `20rb bensin`, `1.5jt kos`, `+5jt gaji`
- 🏷️ Pilih kategori sekali tap (inline keyboard)
- 📊 `/hariini` dan `/bulanini` — rekap per kategori + total
- 🕐 `/riwayat` — 5 transaksi terakhir
- 🔒 Owner-only — orang lain yang chat bot di-ignore total
- 🌐 Dashboard web read-only — grafik harian, breakdown kategori, riwayat transaksi
- 📱 PWA — bisa di-install ke home screen HP kayak app native

## Setup cepat

1. Bikin bot di [@BotFather](https://t.me/BotFather), simpan tokennya
2. Cek Telegram user ID kamu via [@userinfobot](https://t.me/userinfobot)
3. Siapkan database:
   ```sql
   CREATE USER keuangan WITH PASSWORD 'passwordkuat';
   CREATE DATABASE keuangan OWNER keuangan;
   ```
4. Copy `.env.example` → `.env`, isi semua
5. Jalankan:
   ```bash
   cargo run
   ```
   Migrations jalan otomatis saat startup.

## Env vars

Wajib:

- `TELOXIDE_TOKEN` — token dari BotFather
- `OWNER_ID` — Telegram user ID kamu
- `DATABASE_URL` — koneksi Postgres

Opsional (dashboard):

- `DASHBOARD_PASSWORD` — password login. Kosong = dashboard terbuka tanpa auth (dev only)
- `SESSION_SECRET` — kunci HMAC buat cookie sesi. String random >=32 char, **jangan diganti** setelah deploy (bikin sesi login lama invalid)
- `BIND_ADDR` — default `0.0.0.0:8765`

Generate `SESSION_SECRET`:

```bash
openssl rand -base64 48
```

## Dashboard web

- Buka `http://host:8765` → auto-redirect ke `/login` → masukin password → cookie sesi 30 hari
- Endpoint: `/` (HTML), `/login`, `/logout`, `/api/summary?month=YYYY-MM` (JSON)
- **Read-only**: cuma SELECT, nol operasi tulis — aman dijadiin snapshot bulanan
- Timezone: WIB (Asia/Jakarta)

**Install ke home screen HP:**

1. Buka dashboard di Chrome (Android) atau Safari (iOS)
2. Login sekali
3. Menu browser → **"Add to Home Screen"** / **"Install app"**
4. Icon Buku Kas muncul di home screen — tap = buka standalone (tanpa bar browser)

## Deploy di Coolify

1. Push repo ini ke GitHub
2. Coolify → New Resource → pilih repo, build pack: Dockerfile
3. Set env vars di atas — `DATABASE_URL` pakai hostname internal Postgres di network Coolify
4. Publish port `8765` (atau route via domain + Traefik)

> **RAM terbatas?** Build Rust bisa berat. Kalau OOM, build di GitHub Actions dan deploy prebuilt image.

## Cara pakai

| Input | Hasil |
|---|---|
| `15k soto` | Pengeluaran Rp15.000, note "soto" |
| `bensin 20rb` | Nominal bisa di mana aja |
| `1.5jt kos` | Suffix jt/juta didukung |
| `15.000 kopi` | Format titik ribuan juga bisa |
| `+5jt gaji` | Prefix `+` = pemasukan |
