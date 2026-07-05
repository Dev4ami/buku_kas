# 💰 keuangan-bot

Bot Telegram pribadi buat catat pengeluaran tanpa ribet. Ketik `15k soto`, tap kategori, selesai.

## Fitur

- ⚡ Input super cepat: `15k soto`, `20rb bensin`, `1.5jt kos`, `+5jt gaji`
- 🏷️ Pilih kategori sekali tap (inline keyboard)
- 📊 `/hariini` dan `/bulanini` — rekap per kategori + total
- 🕐 `/riwayat` — 5 transaksi terakhir
- 🔒 Owner-only — orang lain yang chat bot di-ignore total

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

## Deploy di Coolify

1. Push repo ini ke GitHub
2. Coolify → New Resource → pilih repo, build pack: Dockerfile
3. Set env vars (`TELOXIDE_TOKEN`, `OWNER_ID`, `DATABASE_URL`)
4. `DATABASE_URL` pakai hostname internal Postgres di network Coolify

> **RAM terbatas?** Build Rust bisa berat. Kalau OOM, build di GitHub Actions dan deploy prebuilt image (sama kayak kasus 9router).

## Cara pakai

| Input | Hasil |
|---|---|
| `15k soto` | Pengeluaran Rp15.000, note "soto" |
| `bensin 20rb` | Nominal bisa di mana aja |
| `1.5jt kos` | Suffix jt/juta didukung |
| `15.000 kopi` | Format titik ribuan juga bisa |
| `+5jt gaji` | Prefix `+` = pemasukan |
