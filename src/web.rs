//! Modul web dashboard — jalan sebagai task tokio berdampingan dengan bot.
//! Read-only: cuma SELECT, nol operasi tulis.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, Json},
    routing::get,
    Router,
};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use chrono_tz::Asia::Jakarta;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

const INDEX_HTML: &str = include_str!("../static/index.html");
const CHART_JS: &str = include_str!("../static/chart.umd.js");

#[derive(Clone)]
struct WebState {
    pool: PgPool,
    token: Option<String>,
}

#[derive(Deserialize)]
struct SummaryParams {
    month: Option<String>,
    token: Option<String>,
}

#[derive(Serialize)]
struct Summary {
    month: String,
    income: i64,
    expense: i64,
    by_category: Vec<CategorySum>,
    daily: Vec<DailySum>,
    transactions: Vec<Tx>,
}

#[derive(Serialize, sqlx::FromRow)]
struct CategorySum {
    name: String,
    emoji: String,
    total: i64,
}

#[derive(Serialize, sqlx::FromRow)]
struct DailySum {
    day: i32,
    total: i64,
}

#[derive(Serialize, sqlx::FromRow)]
struct Tx {
    id: i64,
    amount: i64,
    note: Option<String>,
    category: Option<String>,
    emoji: Option<String>,
    tx_type: String,
    created_at: DateTime<Utc>,
}

/// Jalankan web server. Dipanggil via tokio::spawn dari main.
pub async fn serve(pool: PgPool) {
    let token = std::env::var("DASHBOARD_TOKEN").ok().filter(|t| !t.is_empty());
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8765".into());

    let state = WebState { pool, token };
    let app = Router::new()
        .route("/", get(index))
        .route("/chart.js", get(chart_js))
        .route("/api/summary", get(summary))
        .with_state(state);

    match tokio::net::TcpListener::bind(&bind).await {
        Ok(listener) => {
            tracing::info!("Dashboard jalan di http://{bind} 📊");
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("Web server berhenti: {e}");
            }
        }
        Err(e) => tracing::error!("Gagal bind {bind}: {e} — dashboard tidak jalan, bot tetap lanjut"),
    }
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn chart_js() -> ([(&'static str, &'static str); 2], &'static str) {
    (
        [
            ("content-type", "application/javascript; charset=utf-8"),
            ("cache-control", "public, max-age=86400"),
        ],
        CHART_JS,
    )
}

fn month_range(month: Option<&str>) -> Option<(DateTime<Utc>, DateTime<Utc>, String)> {
    let now = Utc::now().with_timezone(&Jakarta);
    let (y, m) = match month {
        Some(s) => {
            let (ys, ms) = s.split_once('-')?;
            (ys.parse().ok()?, ms.parse().ok()?)
        }
        None => (now.year(), now.month()),
    };
    if !(1..=12).contains(&m) {
        return None;
    }
    let start = Jakarta.with_ymd_and_hms(y, m, 1, 0, 0, 0).single()?;
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let end = Jakarta.with_ymd_and_hms(ny, nm, 1, 0, 0, 0).single()?;
    Some((
        start.with_timezone(&Utc),
        end.with_timezone(&Utc),
        format!("{:04}-{:02}", y, m),
    ))
}

async fn summary(
    State(state): State<WebState>,
    Query(params): Query<SummaryParams>,
) -> Result<Json<Summary>, StatusCode> {
    if let Some(expected) = &state.token {
        if params.token.as_deref() != Some(expected.as_str()) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    let (from, to, label) =
        month_range(params.month.as_deref()).ok_or(StatusCode::BAD_REQUEST)?;
    let pool = &state.pool;
    let err = |_| StatusCode::INTERNAL_SERVER_ERROR;

    let (income, expense): (Option<i64>, Option<i64>) = sqlx::query_as(
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
    .await
    .map_err(err)?;

    let by_category = sqlx::query_as::<_, CategorySum>(
        r#"
        SELECT COALESCE(c.name, 'tanpa kategori') AS name,
               COALESCE(c.emoji, '❓') AS emoji,
               SUM(t.amount)::BIGINT AS total
        FROM transactions t
        LEFT JOIN categories c ON c.id = t.category_id
        WHERE t.tx_type = 'expense' AND t.created_at >= $1 AND t.created_at < $2
        GROUP BY c.name, c.emoji
        ORDER BY total DESC
        "#,
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await
    .map_err(err)?;

    let daily = sqlx::query_as::<_, DailySum>(
        r#"
        SELECT EXTRACT(DAY FROM (created_at AT TIME ZONE 'Asia/Jakarta'))::INT AS day,
               SUM(amount)::BIGINT AS total
        FROM transactions
        WHERE tx_type = 'expense' AND created_at >= $1 AND created_at < $2
        GROUP BY day
        ORDER BY day
        "#,
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await
    .map_err(err)?;

    let transactions = sqlx::query_as::<_, Tx>(
        r#"
        SELECT t.id, t.amount, t.note, c.name AS category, c.emoji AS emoji,
               t.tx_type, t.created_at
        FROM transactions t
        LEFT JOIN categories c ON c.id = t.category_id
        WHERE t.created_at >= $1 AND t.created_at < $2
        ORDER BY t.created_at DESC
        LIMIT 100
        "#,
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await
    .map_err(err)?;

    Ok(Json(Summary {
        month: label,
        income: income.unwrap_or(0),
        expense: expense.unwrap_or(0),
        by_category,
        daily,
        transactions,
    }))
}
