//! keuangan-bot — pencatat keuangan pribadi via Telegram.
//!
//! Flow utama:
//!   1. Kirim "15k soto" → bot parse nominal, insert tx, balas inline keyboard kategori
//!   2. Tap kategori → tx di-update, pesan di-edit jadi konfirmasi
//!   3. /hariini & /bulanini → laporan
//!
//! Keamanan: hanya OWNER_ID yang dilayani. Ini bot pribadi.

mod db;
mod parser;
mod report;
mod web;

use anyhow::Result;
use sqlx::PgPool;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode},
    utils::command::BotCommands,
};

use parser::format_rupiah;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Perintah yang tersedia:")]
enum Command {
    #[command(description = "mulai / bantuan")]
    Start,
    #[command(description = "laporan hari ini")]
    Hariini,
    #[command(description = "laporan bulan ini")]
    Bulanini,
    #[command(description = "5 transaksi terakhir")]
    Riwayat,
    #[command(description = "hapus transaksi, contoh: /hapus 2")]
    Hapus(String),
}

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    owner_id: i64,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL wajib di-set");
    let owner_id: i64 = std::env::var("OWNER_ID")
        .expect("OWNER_ID wajib di-set (Telegram user id kamu)")
        .parse()
        .expect("OWNER_ID harus angka");

    let pool = db::connect(&database_url).await?;
    tracing::info!("Database connected & migrated");

    // Dashboard web jalan berdampingan sebagai task terpisah
    tokio::spawn(web::serve(pool.clone()));

    let bot = Bot::from_env(); // baca TELOXIDE_TOKEN
    let state = AppState { pool, owner_id };

    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(handle_command),
        )
        .branch(Update::filter_message().endpoint(handle_text))
        .branch(Update::filter_callback_query().endpoint(handle_callback));

    tracing::info!("Bot jalan 🚀");
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}

fn is_owner(state: &AppState, user_id: Option<UserId>) -> bool {
    user_id.map(|u| u.0 as i64 == state.owner_id).unwrap_or(false)
}

async fn handle_command(bot: Bot, msg: Message, cmd: Command, state: AppState) -> Result<()> {
    if !is_owner(&state, msg.from.as_ref().map(|u| u.id)) {
        return Ok(()); // diam aja ke orang lain
    }

    let text = match cmd {
        Command::Start => concat!(
            "Halo! Aku pencatat keuanganmu 💰\n\n",
            "Cara pakai:\n",
            "• `15k soto` → catat pengeluaran\n",
            "• `+5jt gaji` → catat pemasukan\n",
            "• /hariini → rekap hari ini\n",
            "• /bulanini → rekap bulan ini\n",
            "• /riwayat → transaksi terakhir\n",
            "• /hapus 2 → hapus transaksi #2"
        )
        .to_string(),
        Command::Hariini => report::today(&state.pool).await?,
        Command::Bulanini => report::this_month(&state.pool).await?,
        Command::Riwayat => {
            let txs = db::recent_txs(&state.pool, 5).await?;
            if txs.is_empty() {
                "Belum ada transaksi.".to_string()
            } else {
                let mut out = String::from("🕐 *Transaksi terakhir:*\n\n");
                for t in txs {
                    out.push_str(&format!(
                        "`#{}` {} — {} ({})\n",
                        t.id,
                        format_rupiah(t.amount),
                        t.note.unwrap_or_else(|| "-".into()),
                        t.category.unwrap_or_else(|| "?".into()),
                    ));
                }
                out
            }
        }
        Command::Hapus(arg) => {
            match arg.trim().trim_start_matches('#').parse::<i64>() {
                Ok(tx_id) => {
                    if db::delete_tx(&state.pool, tx_id).await? {
                        format!("🗑 Transaksi #{} dihapus.", tx_id)
                    } else {
                        format!("Transaksi #{} gak ketemu 🤔 Cek /riwayat", tx_id)
                    }
                }
                Err(_) => "Formatnya: `/hapus 2` (angka dari /riwayat)".to_string(),
            }
        }
    };

    bot.send_message(msg.chat.id, text)
        .parse_mode(ParseMode::Markdown)
        .await?;
    Ok(())
}

async fn handle_text(bot: Bot, msg: Message, state: AppState) -> Result<()> {
    if !is_owner(&state, msg.from.as_ref().map(|u| u.id)) {
        return Ok(());
    }
    let Some(text) = msg.text() else { return Ok(()) };

    let Some(parsed) = parser::parse(text) else {
        bot.send_message(
            msg.chat.id,
            "Hmm, gak nemu nominalnya 🤔 Coba format kayak `15k soto`",
        )
        .parse_mode(ParseMode::Markdown)
        .await?;
        return Ok(());
    };

    let tx_type = if parsed.is_income { "income" } else { "expense" };
    let tx_id = db::insert_tx(&state.pool, parsed.amount, &parsed.note, tx_type).await?;

    if parsed.is_income {
        bot.send_message(
            msg.chat.id,
            format!("💰 Pemasukan {} tercatat!", format_rupiah(parsed.amount)),
        )
        .await?;
        return Ok(());
    }

    // Pengeluaran → tawarkan kategori via inline keyboard
    let categories = db::list_categories(&state.pool).await?;
    let buttons: Vec<Vec<InlineKeyboardButton>> = categories
        .chunks(3)
        .map(|chunk| {
            chunk
                .iter()
                .map(|c| {
                    InlineKeyboardButton::callback(
                        format!("{} {}", c.emoji, c.name),
                        format!("cat:{}:{}", tx_id, c.id),
                    )
                })
                .collect()
        })
        .collect();

    let mut keyboard = buttons;
    keyboard.push(vec![InlineKeyboardButton::callback(
        "🗑 batal".to_string(),
        format!("del:{}", tx_id),
    )]);

    bot.send_message(
        msg.chat.id,
        format!(
            "💸 {} — {}\nKategorinya apa nih?",
            format_rupiah(parsed.amount),
            if parsed.note.is_empty() { "(tanpa catatan)" } else { &parsed.note }
        ),
    )
    .reply_markup(InlineKeyboardMarkup::new(keyboard))
    .await?;

    Ok(())
}

async fn handle_callback(bot: Bot, q: CallbackQuery, state: AppState) -> Result<()> {
    if q.from.id.0 as i64 != state.owner_id {
        return Ok(());
    }
    let Some(data) = q.data.as_deref() else { return Ok(()) };

    if let Some(rest) = data.strip_prefix("cat:") {
        // format: cat:<tx_id>:<category_id>
        let mut parts = rest.split(':');
        let tx_id: i64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let cat_id: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);

        db::set_tx_category(&state.pool, tx_id, cat_id).await?;

        if let Some(msg) = q.regular_message() {
            let original = msg.text().unwrap_or("").lines().next().unwrap_or("").to_string();
            bot.edit_message_text(msg.chat.id, msg.id, format!("✅ {} — tercatat!", original))
                .await?;
        }
        bot.answer_callback_query(q.id).await?;
    } else if let Some(tx_id_str) = data.strip_prefix("del:") {
        let tx_id: i64 = tx_id_str.parse().unwrap_or(0);
        db::delete_tx(&state.pool, tx_id).await?;

        if let Some(msg) = q.regular_message() {
            bot.edit_message_text(msg.chat.id, msg.id, "🗑 Dibatalkan.").await?;
        }
        bot.answer_callback_query(q.id).await?;
    }

    Ok(())
}
