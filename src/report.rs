//! Generator laporan teks buat /hariini dan /bulanini.
//! Semua rentang waktu dihitung di zona WIB (Asia/Jakarta).

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use chrono_tz::Asia::Jakarta;
use sqlx::PgPool;

use crate::db;
use crate::parser::format_rupiah;

/// Rentang hari ini dalam WIB → (from, to) sebagai UTC
fn today_range() -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now().with_timezone(&Jakarta);
    let start = Jakarta
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .unwrap();
    (start.with_timezone(&Utc), (start + Duration::days(1)).with_timezone(&Utc))
}

/// Rentang bulan ini dalam WIB → (from, to) sebagai UTC
fn month_range() -> (DateTime<Utc>, DateTime<Utc>) {
    let now = Utc::now().with_timezone(&Jakarta);
    let start = Jakarta
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .unwrap();
    let (ny, nm) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    let end = Jakarta.with_ymd_and_hms(ny, nm, 1, 0, 0, 0).unwrap();
    (start.with_timezone(&Utc), end.with_timezone(&Utc))
}

async fn build_report(pool: &PgPool, from: DateTime<Utc>, to: DateTime<Utc>, title: &str) -> Result<String> {
    let sums = db::sum_by_category(pool, from, to).await?;
    let (income, expense) = db::totals(pool, from, to).await?;

    let mut out = format!("📊 *{}*\n\n", title);

    if sums.is_empty() {
        out.push_str("Belum ada pengeluaran. Dompet aman... untuk sekarang 😌\n");
    } else {
        for row in &sums {
            out.push_str(&format!(
                "{} {} — {}\n",
                row.emoji,
                row.name,
                format_rupiah(row.total)
            ));
        }
        out.push('\n');
    }

    out.push_str(&format!("💸 Total keluar: {}\n", format_rupiah(expense)));
    if income > 0 {
        out.push_str(&format!("💰 Total masuk: {}\n", format_rupiah(income)));
        let net = income - expense;
        let sign = if net >= 0 { "✅" } else { "🔻" };
        out.push_str(&format!("{} Selisih: {}\n", sign, format_rupiah(net.abs())));
    }

    Ok(out)
}

pub async fn today(pool: &PgPool) -> Result<String> {
    let (from, to) = today_range();
    build_report(pool, from, to, "Hari Ini").await
}

pub async fn this_month(pool: &PgPool) -> Result<String> {
    let (from, to) = month_range();
    build_report(pool, from, to, "Bulan Ini").await
}
