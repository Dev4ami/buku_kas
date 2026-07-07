//! Modul web dashboard — jalan sebagai task tokio berdampingan dengan bot.
//! Read-only: cuma SELECT, nol operasi tulis.
//!
//! Auth: password (DASHBOARD_PASSWORD) + session cookie (HMAC dari SESSION_SECRET).
//! Kalau DASHBOARD_PASSWORD kosong → dashboard terbuka tanpa auth.

use axum::{
    extract::{Form, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Json, Redirect, Response},
    routing::get,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use chrono_tz::Asia::Jakarta;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::PgPool;

type HmacSha256 = Hmac<Sha256>;

const INDEX_HTML: &str = include_str!("../static/index.html");
const CHART_JS: &str = include_str!("../static/chart.umd.js");

const SESSION_COOKIE: &str = "dashboard_session";
const SESSION_MAX_AGE_SECS: i64 = 60 * 60 * 24 * 30; // 30 hari

#[derive(Clone)]
struct WebState {
    pool: PgPool,
    /// Password login. None = auth dimatikan (semua request diizinkan).
    password: Option<String>,
    /// Nilai cookie yang valid (HMAC-SHA256 dari SESSION_SECRET). None kalau auth mati.
    session_token: Option<String>,
}

#[derive(Deserialize)]
struct SummaryParams {
    month: Option<String>,
}

#[derive(Deserialize)]
struct LoginForm {
    password: String,
}

#[derive(Deserialize)]
struct LoginQuery {
    error: Option<String>,
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

fn compute_session_token(secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(b"dashboard-authorized-v1");
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (k, v) = part.trim().split_once('=')?;
            (k == name).then_some(v)
        })
}

fn is_authorized(state: &WebState, headers: &HeaderMap) -> bool {
    let Some(expected) = &state.session_token else {
        return true; // auth mati
    };
    cookie_value(headers, SESSION_COOKIE).is_some_and(|c| c == expected)
}

/// Jalankan web server. Dipanggil via tokio::spawn dari main.
pub async fn serve(pool: PgPool) {
    let password = std::env::var("DASHBOARD_PASSWORD")
        .ok()
        .filter(|s| !s.is_empty());
    let secret = std::env::var("SESSION_SECRET").ok().filter(|s| !s.is_empty());
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8765".into());

    let session_token = match (&password, &secret) {
        (Some(_), Some(sec)) => Some(compute_session_token(sec)),
        (Some(_), None) => {
            tracing::error!(
                "DASHBOARD_PASSWORD di-set tapi SESSION_SECRET kosong — auth dimatikan"
            );
            None
        }
        _ => None,
    };

    if session_token.is_none() && password.is_none() {
        tracing::warn!("Dashboard jalan TANPA auth (DASHBOARD_PASSWORD kosong)");
    }

    let state = WebState {
        pool,
        password,
        session_token,
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", get(logout))
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
        Err(e) => tracing::error!(
            "Gagal bind {bind}: {e} — dashboard tidak jalan, bot tetap lanjut"
        ),
    }
}

async fn index(State(state): State<WebState>, headers: HeaderMap) -> Response {
    if !is_authorized(&state, &headers) {
        return Redirect::to("/login").into_response();
    }
    Html(INDEX_HTML).into_response()
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

async fn login_page(State(state): State<WebState>, Query(q): Query<LoginQuery>) -> Response {
    // Kalau auth mati, redirect ke dashboard
    if state.password.is_none() {
        return Redirect::to("/").into_response();
    }
    let error_html = if q.error.is_some() {
        r#"<p class="err">Password salah.</p>"#
    } else {
        ""
    };
    Html(LOGIN_HTML.replace("{ERROR}", error_html)).into_response()
}

async fn login_submit(
    State(state): State<WebState>,
    Form(form): Form<LoginForm>,
) -> Response {
    let (Some(expected_pw), Some(token)) = (&state.password, &state.session_token) else {
        return Redirect::to("/").into_response();
    };

    if form.password != *expected_pw {
        return Redirect::to("/login?error=1").into_response();
    }

    let cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; Max-Age={SESSION_MAX_AGE_SECS}; HttpOnly; SameSite=Lax"
    );
    (
        StatusCode::SEE_OTHER,
        [(header::SET_COOKIE, cookie), (header::LOCATION, "/".into())],
    )
        .into_response()
}

async fn logout() -> Response {
    let cookie = format!("{SESSION_COOKIE}=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax");
    (
        StatusCode::SEE_OTHER,
        [
            (header::SET_COOKIE, cookie),
            (header::LOCATION, "/login".into()),
        ],
    )
        .into_response()
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
    headers: HeaderMap,
    Query(params): Query<SummaryParams>,
) -> Result<Json<Summary>, StatusCode> {
    if !is_authorized(&state, &headers) {
        return Err(StatusCode::UNAUTHORIZED);
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

const LOGIN_HTML: &str = r#"<!DOCTYPE html>
<html lang="id">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Login — Buku Kas</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@500;700&family=IBM+Plex+Mono:wght@400;500;600&display=swap" rel="stylesheet">
<style>
  :root {
    --bg: #F1F4F1;
    --ink: #171B18;
    --muted: #7C837E;
    --paper: #FFFFFF;
    --line: #E1E6E1;
    --red: #C4452F;
    --teal: #167C80;
  }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body {
    background: var(--bg);
    color: var(--ink);
    font-family: 'Space Grotesk', sans-serif;
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  .card {
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 28px 26px 24px;
    width: 100%;
    max-width: 340px;
    box-shadow: 0 2px 10px rgba(23,27,24,0.06);
  }
  h1 {
    font-size: 20px; font-weight: 700; letter-spacing: -0.02em;
    margin-bottom: 4px;
  }
  h1 .dot { color: var(--teal); }
  .sub {
    font-size: 13px; color: var(--muted); margin-bottom: 20px;
  }
  label {
    display: block; font-size: 11px; text-transform: uppercase;
    letter-spacing: 0.08em; color: var(--muted); margin-bottom: 6px;
  }
  input[type=password] {
    width: 100%;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 14px;
    border: 1.5px solid var(--ink);
    background: var(--paper);
    padding: 10px 12px;
    border-radius: 6px;
    color: var(--ink);
  }
  input[type=password]:focus {
    outline: 2px solid var(--teal);
    outline-offset: 1px;
  }
  button {
    margin-top: 14px;
    width: 100%;
    font-family: 'Space Grotesk', sans-serif;
    font-weight: 700;
    font-size: 14px;
    background: var(--ink);
    color: var(--paper);
    border: none;
    padding: 11px 12px;
    border-radius: 6px;
    cursor: pointer;
    letter-spacing: 0.02em;
  }
  button:hover { background: var(--teal); }
  .err {
    margin-top: 12px;
    font-size: 13px;
    color: var(--red);
    font-family: 'IBM Plex Mono', monospace;
  }
</style>
</head>
<body>
  <form class="card" method="POST" action="/login" autocomplete="on">
    <h1>Buku Kas<span class="dot">.</span></h1>
    <p class="sub">Dashboard read-only</p>
    <label for="password">Password</label>
    <input type="password" id="password" name="password" autofocus required>
    <button type="submit">Masuk</button>
    {ERROR}
  </form>
</body>
</html>"#;
