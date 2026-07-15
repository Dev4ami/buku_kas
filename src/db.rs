//! Layer database. Semua query lewat sini biar handlers tetap bersih.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

#[derive(Debug, sqlx::FromRow)]
pub struct Category {
    pub id: i32,
    pub name: String,
    pub emoji: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct CategorySum {
    pub name: String,
    pub emoji: String,
    pub total: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct TxRow {
    pub id: i64,
    pub amount: i64,
    pub note: Option<String>,
    pub category: Option<String>,
}

pub async fn connect(database_url: &str) -> Result<PgPool> {
    let pool = PgPool::connect(database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub async fn list_categories(pool: &PgPool) -> Result<Vec<Category>> {
    let rows = sqlx::query_as::<_, Category>(
        "SELECT id, name, emoji FROM categories ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Insert transaksi, return id-nya (buat update kategori setelah user tap tombol)
pub async fn insert_tx(
    pool: &PgPool,
    amount: i64,
    note: &str,
    tx_type: &str,
) -> Result<i64> {
    let rec: (i64,) = sqlx::query_as(
        "INSERT INTO transactions (amount, note, tx_type) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(amount)
    .bind(if note.is_empty() { None } else { Some(note) })
    .bind(tx_type)
    .fetch_one(pool)
    .await?;
    Ok(rec.0)
}

pub async fn set_tx_category(pool: &PgPool, tx_id: i64, category_id: i32) -> Result<()> {
    sqlx::query("UPDATE transactions SET category_id = $1 WHERE id = $2")
        .bind(category_id)
        .bind(tx_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_tx(pool: &PgPool, tx_id: i64) -> Result<bool> {
    let res = sqlx::query("DELETE FROM transactions WHERE id = $1")
        .bind(tx_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// Total pengeluaran per kategori dalam rentang waktu
pub async fn sum_by_category(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<CategorySum>> {
    let rows = sqlx::query_as::<_, CategorySum>(
        r#"
        SELECT COALESCE(c.name, 'tanpa kategori') AS name,
               COALESCE(c.emoji, '❓') AS emoji,
               SUM(t.amount)::BIGINT AS total
        FROM transactions t
        LEFT JOIN categories c ON c.id = t.category_id
        WHERE t.tx_type = 'expense'
          AND t.created_at >= $1 AND t.created_at < $2
        GROUP BY c.name, c.emoji
        ORDER BY total DESC
        "#,
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Total income & expense dalam rentang waktu → (income, expense)
pub async fn totals(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<(i64, i64)> {
    let rec: (Option<i64>, Option<i64>) = sqlx::query_as(
        r#"
        SELECT
            (SUM(amount) FILTER (WHERE tx_type = 'income'))::BIGINT,
            (SUM(amount) FILTER (WHERE tx_type = 'expense'))::BIGINT
        FROM transactions
        WHERE created_at >= $1 AND created_at < $2
        "#,
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    Ok((rec.0.unwrap_or(0), rec.1.unwrap_or(0)))
}

/// Transaksi terakhir (buat /riwayat)
pub async fn recent_txs(pool: &PgPool, limit: i64) -> Result<Vec<TxRow>> {
    let rows = sqlx::query_as::<_, TxRow>(
        r#"
        SELECT t.id, t.amount, t.note, c.name AS category
        FROM transactions t
        LEFT JOIN categories c ON c.id = t.category_id
        ORDER BY t.created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
