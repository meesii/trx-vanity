use chrono::Utc;
use k256::{elliptic_curve::sec1::ToEncodedPoint, SecretKey};
use rand_core::OsRng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tiny_keccak::{Hasher, Keccak};
use uuid::Uuid;
use zeroize::Zeroize;

#[derive(Clone)]
struct AppState {
    stop_signal: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct RuleConfig {
    match_types: Vec<MatchType>,
    pattern_ids: Vec<String>,
    target_count: u32,
    max_attempts: u64,
    threads: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum MatchType {
    Prefix,
    Suffix,
    Contains,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct WalletItem {
    id: String,
    address: String,
    private_key: String,
    rule_label: String,
    attempts: u64,
    matched: bool,
    matched_parts: Vec<String>,
    matched_rules: Vec<String>,
    created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct ProgressEvent {
    status: String,
    attempts: u64,
    found_count: u32,
    target_count: u32,
    speed: u64,
    latest_address: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct WalletBatch {
    wallets: Vec<WalletItem>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct GenerateResult {
    wallets: Vec<WalletItem>,
    attempts: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
struct SearchConfig {
    keyword: String,
    limit: Option<u32>,
    matched_only: Option<bool>,
}

#[derive(Clone)]
struct WalletData {
    address: String,
    private_key: String,
}

/**
 * 快速路径中间产物，仅包含地址字符串和原始私钥字节
 * 避免在未匹配时执行 hex 编码
 */
struct RawWallet {
    address: String,
    secret_bytes: [u8; 32],
}

#[derive(Default)]
struct MatchResult {
    matched_parts: Vec<String>,
    matched_rules: Vec<String>,
}

#[derive(Clone)]
struct PatternDef {
    label: String,
    kind: PatternKind,
}

#[derive(Clone)]
enum PatternKind {
    Shape(String),
    Straight(usize),
    Exact(String),
}

#[tauri::command]
async fn generate_wallets(
    app: AppHandle,
    state: State<'_, AppState>,
    config: RuleConfig,
) -> Result<GenerateResult, String> {
    validate_config(&config)?;

    let stop_signal = state.stop_signal.clone();
    stop_signal.store(false, Ordering::Relaxed);

    tauri::async_runtime::spawn_blocking(move || {
        generate_wallets_blocking(app, stop_signal, config)
    })
    .await
    .map_err(|err| format!("生成任务执行失败：{err}"))?
}

#[tauri::command]
fn stop_wallets(state: State<'_, AppState>) {
    state.stop_signal.store(true, Ordering::Relaxed);
}

#[tauri::command]
fn cpu_count() -> usize {
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

#[tauri::command]
fn list_wallets(app: AppHandle) -> Result<Vec<WalletItem>, String> {
    read_wallets(&app, "", 500, None)
}

#[tauri::command]
fn search_wallets(app: AppHandle, config: SearchConfig) -> Result<Vec<WalletItem>, String> {
    read_wallets(
        &app,
        config.keyword.trim(),
        config.limit.unwrap_or(500),
        config.matched_only,
    )
}

#[tauri::command]
fn delete_wallet(app: AppHandle, id: String) -> Result<(), String> {
    init_database(&app)?;
    let conn = open_database(&app)?;
    conn.execute("DELETE FROM wallets WHERE id = ?1", params![id])
        .map_err(|err| format!("删除钱包记录失败：{err}"))?;
    Ok(())
}

#[tauri::command]
fn clear_wallets(app: AppHandle) -> Result<(), String> {
    init_database(&app)?;
    let conn = open_database(&app)?;
    conn.execute("DELETE FROM wallets", [])
        .map_err(|err| format!("清空钱包记录失败：{err}"))?;
    Ok(())
}

#[tauri::command]
fn export_wallets(app: AppHandle, path: String) -> Result<u32, String> {
    init_database(&app)?;
    let conn = open_database(&app)?;
    let mut stmt = conn
        .prepare("SELECT address, private_key FROM wallets ORDER BY created_at DESC")
        .map_err(|e| format!("查询失败：{e}"))?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("读取失败：{e}"))?
        .filter_map(|r| r.ok())
        .collect();
    let count = rows.len() as u32;
    let mut content = String::with_capacity(count as usize * 120);
    for (addr, key) in &rows {
        content.push_str(addr);
        content.push_str("----");
        content.push_str(key);
        content.push('\n');
    }
    std::fs::write(&path, content).map_err(|e| format!("写入文件失败：{e}"))?;
    Ok(count)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            stop_signal: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(tauri::generate_handler![
            generate_wallets,
            stop_wallets,
            cpu_count,
            list_wallets,
            search_wallets,
            delete_wallet,
            clear_wallets,
            export_wallets
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}

fn generate_wallets_blocking(
    app: AppHandle,
    stop_signal: Arc<AtomicBool>,
    config: RuleConfig,
) -> Result<GenerateResult, String> {
    let max_threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let num_threads = config.threads.unwrap_or(1).clamp(1, max_threads);

    let attempts = Arc::new(AtomicU64::new(0));
    let found_count = Arc::new(AtomicU32::new(0));
    let matched_wallets: Arc<Mutex<Vec<WalletItem>>> = Arc::new(Mutex::new(Vec::new()));
    let pending_wallets: Arc<Mutex<Vec<WalletItem>>> = Arc::new(Mutex::new(Vec::new()));
    let latest_addr: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let start_time = Instant::now();
    let label = rule_label(&config);

    init_database(&app)?;

    eprintln!("[DEBUG] 启动 {} 个工作线程", num_threads);

    thread::scope(|s| {
        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let stop = stop_signal.clone();
                let attempts = attempts.clone();
                let found = found_count.clone();
                let matched_w = matched_wallets.clone();
                let pending_w = pending_wallets.clone();
                let latest_a = latest_addr.clone();
                let config = config.clone();
                let label = label.clone();

                s.spawn(move || {
                    let mut local_pending = Vec::new();
                    let mut last_push = Instant::now();

                    loop {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }

                        let n = attempts.fetch_add(1, Ordering::Relaxed) + 1;
                        let mut raw = create_wallet_fast();
                        let match_result = find_matches(&raw.address, &config);
                        let matched = !match_result.matched_parts.is_empty();

                        if matched {
                            found.fetch_add(1, Ordering::Relaxed);
                            let wallet_data = finalize_wallet(raw);

                            let wallet = WalletItem {
                                id: Uuid::new_v4().to_string(),
                                address: wallet_data.address,
                                private_key: wallet_data.private_key,
                                rule_label: label.clone(),
                                attempts: n,
                                matched: true,
                                matched_parts: match_result.matched_parts,
                                matched_rules: match_result.matched_rules,
                                created_at: Utc::now().to_rfc3339(),
                            };

                            matched_w.lock().unwrap().push(wallet.clone());
                            local_pending.push(wallet);
                        } else {
                            *latest_a.lock().unwrap() = Some(raw.address.clone());
                            raw.secret_bytes.zeroize();
                        }

                        if local_pending.len() >= 64 || last_push.elapsed() >= Duration::from_millis(100) || matched {
                            if !local_pending.is_empty() {
                                pending_w.lock().unwrap().append(&mut local_pending);
                            }
                            last_push = Instant::now();
                        }

                        if config.target_count > 0 && found.load(Ordering::Relaxed) >= config.target_count {
                            stop.store(true, Ordering::Relaxed);
                        }
                    }

                    if !local_pending.is_empty() {
                        pending_w.lock().unwrap().append(&mut local_pending);
                    }
                })
            })
            .collect();

        /* 主线程负责 emit + flush */
        let mut last_emit = Instant::now();
        let mut last_flush = Instant::now();

        eprintln!("[DEBUG] 主线程 emit 循环开始, stop_signal={}", stop_signal.load(Ordering::Relaxed));

        let mut flush_buf: Vec<WalletItem> = Vec::new();

        loop {
            thread::sleep(Duration::from_millis(60));

            let cur_attempts = attempts.load(Ordering::Relaxed);
            let cur_found = found_count.load(Ordering::Relaxed);
            let stopped = stop_signal.load(Ordering::Relaxed);

            if stopped || last_emit.elapsed() >= Duration::from_millis(150) {
                let batch: Vec<WalletItem> = {
                    let mut lock = pending_wallets.lock().unwrap();
                    lock.drain(..).collect()
                };
                let latest = batch.last().map(|w| w.address.clone())
                    .or_else(|| latest_addr.lock().unwrap().clone());
                let _ = emit_wallet_batch(&app, &batch);
                let _ = emit_progress(&app, "running", cur_attempts, cur_found, config.target_count, start_time, latest);
                last_emit = Instant::now();

                flush_buf.extend(batch);
            }

            if stopped || last_flush.elapsed() >= Duration::from_millis(600) {
                if !flush_buf.is_empty() {
                    let _ = flush_wallets(&app, &mut flush_buf);
                    last_flush = Instant::now();
                }
            }

            if stopped {
                break;
            }
        }

        for h in handles {
            let _ = h.join();
        }
    });

    /* 最终 flush */
    eprintln!("[DEBUG] scope 结束, attempts={}, found={}", attempts.load(Ordering::Relaxed), found_count.load(Ordering::Relaxed));
    let mut remaining = pending_wallets.lock().unwrap().drain(..).collect::<Vec<_>>();
    let _ = emit_wallet_batch(&app, &remaining);
    let _ = flush_wallets(&app, &mut remaining);

    let total_attempts = attempts.load(Ordering::Relaxed);
    let total_found = found_count.load(Ordering::Relaxed);

    let _ = emit_progress(&app, "stopped", total_attempts, total_found, config.target_count, start_time, None);

    let result_wallets = matched_wallets.lock().unwrap().clone();
    Ok(GenerateResult {
        wallets: result_wallets,
        attempts: total_attempts,
    })
}

/**
 * 快速路径：只生成地址字符串，私钥保留为原始字节
 * 栈上 25 字节数组替代 Vec 堆分配
 */
fn create_wallet_fast() -> RawWallet {
    let secret_key = SecretKey::random(&mut OsRng);
    let public_key = secret_key.public_key();
    let point = public_key.to_encoded_point(false);
    let public_bytes = point.as_bytes();
    let secret_bytes: [u8; 32] = secret_key.to_bytes().into();

    let hash = keccak256(&public_bytes[1..]);
    let mut payload = [0u8; 25];
    payload[0] = 0x41;
    payload[1..21].copy_from_slice(&hash[12..]);

    let check = checksum(&payload[..21]);
    payload[21..25].copy_from_slice(&check);

    let address = bs58::encode(&payload).into_string();

    RawWallet {
        address,
        secret_bytes,
    }
}

/**
 * 慢速路径：匹配成功后才将私钥编码为 hex
 */
fn finalize_wallet(mut raw: RawWallet) -> WalletData {
    let private_key = hex::encode(raw.secret_bytes);
    raw.secret_bytes.zeroize();
    WalletData {
        address: raw.address,
        private_key,
    }
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut output = [0_u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(data);
    hasher.finalize(&mut output);
    output
}

fn checksum(payload: &[u8]) -> [u8; 4] {
    let first_hash = Sha256::digest(payload);
    let second_hash = Sha256::digest(first_hash);

    [
        second_hash[0],
        second_hash[1],
        second_hash[2],
        second_hash[3],
    ]
}

fn validate_config(config: &RuleConfig) -> Result<(), String> {
    if config.match_types.is_empty() {
        return Err("至少选择一种匹配类型。".to_string());
    }

    if config.pattern_ids.is_empty() {
        return Err("至少选择一种靓号规则。".to_string());
    }

    Ok(())
}

fn find_matches(address: &str, config: &RuleConfig) -> MatchResult {
    let body = address.strip_prefix('T').unwrap_or(address);
    let mut result = MatchResult::default();

    for match_type in &config.match_types {
        for pattern_id in &config.pattern_ids {
            let Some(pattern) = pattern_def(pattern_id) else {
                continue;
            };

            let segments = find_pattern_segments(body, match_type, &pattern);

            for segment in segments {
                push_unique(&mut result.matched_parts, segment);
                push_unique(
                    &mut result.matched_rules,
                    format!("{} · {}", match_type_label(match_type), pattern.label),
                );
            }
        }
    }

    result
}

fn find_pattern_segments(body: &str, match_type: &MatchType, pattern: &PatternDef) -> Vec<String> {
    let chars = body.chars().collect::<Vec<_>>();

    match match_type {
        MatchType::Prefix => match_at_positions(&chars, pattern, &[0]),
        MatchType::Suffix => {
            let mut positions = Vec::new();
            for len in pattern.lengths() {
                if chars.len() >= len {
                    positions.push(chars.len() - len);
                }
            }
            match_at_positions(&chars, pattern, &positions)
        }
        MatchType::Contains => {
            if chars.len() < pattern.max_len() {
                return Vec::new();
            }
            let positions = (0..chars.len()).collect::<Vec<_>>();
            match_at_positions(&chars, pattern, &positions)
        }
    }
}

fn match_at_positions(chars: &[char], pattern: &PatternDef, positions: &[usize]) -> Vec<String> {
    let mut segments = Vec::new();

    for &start in positions {
        for len in pattern.lengths() {
            if start + len > chars.len() {
                continue;
            }

            let segment = chars[start..start + len].iter().collect::<String>();
            if pattern_matches(&segment, pattern) {
                segments.push(segment);
            }
        }
    }

    segments
}

impl PatternDef {
    fn lengths(&self) -> Vec<usize> {
        match &self.kind {
            PatternKind::Shape(shape) => vec![shape.len()],
            PatternKind::Straight(size) => vec![*size],
            PatternKind::Exact(value) => vec![value.chars().count()],
        }
    }

    fn max_len(&self) -> usize {
        self.lengths().into_iter().max().unwrap_or(0)
    }
}

fn pattern_def(id: &str) -> Option<PatternDef> {
    if let Some(shape) = id.strip_prefix("shape:") {
        if shape.is_empty() || shape.len() > 20 {
            return None;
        }
        return Some(PatternDef {
            label: shape.to_uppercase(),
            kind: PatternKind::Shape(shape.to_string()),
        });
    }

    if let Some(size) = id.strip_prefix("straight:") {
        let size: usize = size.parse().ok()?;
        if !(2..=10).contains(&size) {
            return None;
        }
        return Some(PatternDef {
            label: format!("{size}位顺子"),
            kind: PatternKind::Straight(size),
        });
    }

    if let Some(value) = id.strip_prefix("custom:") {
        if value.is_empty() || value.len() > 20 {
            return None;
        }
        return Some(PatternDef {
            label: value.to_string(),
            kind: PatternKind::Exact(value.to_string()),
        });
    }

    None
}

fn pattern_matches(segment: &str, pattern: &PatternDef) -> bool {
    match &pattern.kind {
        PatternKind::Shape(shape) => shape_matches(segment, shape),
        PatternKind::Straight(size) => is_straight(segment, *size),
        PatternKind::Exact(value) => segment == value,
    }
}

fn shape_matches(segment: &str, shape: &str) -> bool {
    if segment.chars().count() != shape.chars().count() {
        return false;
    }

    let mut map = Vec::<(char, char)>::new();
    /* 0=未定, 1=同向, -1=反向 */
    let mut dir: i8 = 0;

    for (vc, sc) in segment.chars().zip(shape.chars()) {
        if let Some((mv, _)) = map.iter().find(|(_, s)| *s == sc) {
            if *mv != vc {
                return false;
            }
        } else if map.iter().any(|(v, _)| *v == vc) {
            return false;
        } else {
            for (pv, ps) in &map {
                if *ps != sc {
                    let s_asc = sc > *ps;
                    let v_asc = vc > *pv;
                    let d: i8 = if s_asc == v_asc { 1 } else { -1 };
                    if dir == 0 {
                        dir = d;
                    } else if dir != d {
                        return false;
                    }
                }
            }
            map.push((vc, sc));
        }
    }

    true
}

fn is_straight(segment: &str, size: usize) -> bool {
    let chars = segment.chars().collect::<Vec<_>>();
    if chars.len() != size {
        return false;
    }

    let indexes = chars
        .iter()
        .filter_map(|item| item.to_digit(10))
        .collect::<Vec<_>>();

    if indexes.len() != size {
        return false;
    }

    let ascending = indexes.windows(2).all(|pair| pair[1] == pair[0] + 1);
    let descending = indexes.windows(2).all(|pair| pair[0] == pair[1] + 1);

    ascending || descending
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !list.iter().any(|item| item == &value) {
        list.push(value);
    }
}

fn match_type_label(match_type: &MatchType) -> &'static str {
    match match_type {
        MatchType::Prefix => "前缀",
        MatchType::Suffix => "后缀",
        MatchType::Contains => "包含",
    }
}

fn rule_label(config: &RuleConfig) -> String {
    let type_label = config
        .match_types
        .iter()
        .map(match_type_label)
        .collect::<Vec<_>>()
        .join(" / ");
    let pattern_label = config
        .pattern_ids
        .iter()
        .filter_map(|id| pattern_def(id).map(|item| item.label.to_string()))
        .collect::<Vec<_>>()
        .join(" / ");

    format!("{type_label} · {pattern_label}")
}

fn emit_progress(
    app: &AppHandle,
    status: &str,
    attempts: u64,
    found_count: u32,
    target_count: u32,
    start_time: Instant,
    latest_address: Option<String>,
) -> Result<(), String> {
    let seconds = start_time.elapsed().as_secs().max(1);
    let event = ProgressEvent {
        status: status.to_string(),
        attempts,
        found_count,
        target_count,
        speed: attempts / seconds,
        latest_address,
    };

    app.emit("wallet_progress", event)
        .map_err(|err| format!("推送进度失败：{err}"))
}

fn emit_wallet_batch(app: &AppHandle, wallets: &[WalletItem]) -> Result<(), String> {
    if wallets.is_empty() {
        return Ok(());
    }

    let matched: Vec<WalletItem> = wallets.iter().filter(|w| w.matched).cloned().collect();
    let recent: Vec<WalletItem> = wallets
        .iter()
        .rev()
        .filter(|w| !w.matched)
        .take(40_usize.saturating_sub(matched.len()))
        .cloned()
        .collect();

    let mut batch = matched;
    batch.extend(recent);

    app.emit("wallet_batch", WalletBatch { wallets: batch })
        .map_err(|err| format!("推送钱包记录失败：{err}"))
}

fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("获取数据目录失败：{err}"))?;

    std::fs::create_dir_all(&data_dir).map_err(|err| format!("创建数据目录失败：{err}"))?;
    Ok(data_dir.join("wallet_history.sqlite"))
}

fn open_database(app: &AppHandle) -> Result<Connection, String> {
    let path = database_path(app)?;
    let conn = Connection::open(path).map_err(|err| format!("打开 SQLite 失败：{err}"))?;

    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|err| format!("设置 SQLite WAL 失败：{err}"))?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|err| format!("设置 SQLite 同步模式失败：{err}"))?;

    Ok(conn)
}

fn init_database(app: &AppHandle) -> Result<(), String> {
    let conn = open_database(app)?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS wallets (
            id TEXT PRIMARY KEY,
            address TEXT NOT NULL,
            private_key TEXT NOT NULL,
            rule_label TEXT NOT NULL,
            attempts INTEGER NOT NULL,
            matched INTEGER NOT NULL,
            matched_parts TEXT NOT NULL DEFAULT '[]',
            matched_rules TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_wallets_created_at ON wallets(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_wallets_address ON wallets(address);
        CREATE INDEX IF NOT EXISTS idx_wallets_matched ON wallets(matched);
        ",
    )
    .map_err(|err| format!("初始化 SQLite 失败：{err}"))?;

    add_column_if_missing(&conn, "matched_parts", "TEXT NOT NULL DEFAULT '[]'")?;
    add_column_if_missing(&conn, "matched_rules", "TEXT NOT NULL DEFAULT '[]'")?;

    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('wallets') WHERE name = ?1 LIMIT 1",
            params![column],
            |_| Ok(()),
        )
        .optional()
        .map_err(|err| format!("检查 SQLite 字段失败：{err}"))?
        .is_some();

    if !exists {
        conn.execute(
            &format!("ALTER TABLE wallets ADD COLUMN {column} {definition}"),
            [],
        )
        .map_err(|err| format!("迁移 SQLite 字段失败：{err}"))?;
    }

    Ok(())
}

fn flush_wallets(app: &AppHandle, wallets: &mut Vec<WalletItem>) -> Result<(), String> {
    let matched: Vec<&WalletItem> = wallets.iter().filter(|w| w.matched).collect();
    if matched.is_empty() {
        wallets.clear();
        return Ok(());
    }

    let mut conn = open_database(app)?;
    let tx = conn
        .transaction()
        .map_err(|err| format!("开启 SQLite 事务失败：{err}"))?;

    {
        let mut stmt = tx
            .prepare(
                "
                INSERT OR IGNORE INTO wallets
                    (id, address, private_key, rule_label, attempts, matched, matched_parts, matched_rules, created_at)
                VALUES
                    (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ",
            )
            .map_err(|err| format!("准备 SQLite 写入失败：{err}"))?;

        for wallet in &matched {
            stmt.execute(params![
                wallet.id,
                wallet.address,
                wallet.private_key,
                wallet.rule_label,
                wallet.attempts,
                1_i64,
                json_list(&wallet.matched_parts)?,
                json_list(&wallet.matched_rules)?,
                wallet.created_at,
            ])
            .map_err(|err| format!("写入钱包记录失败：{err}"))?;
        }
    }

    tx.commit()
        .map_err(|err| format!("提交 SQLite 事务失败：{err}"))?;
    wallets.clear();
    Ok(())
}

fn read_wallets(
    app: &AppHandle,
    keyword: &str,
    limit: u32,
    matched_only: Option<bool>,
) -> Result<Vec<WalletItem>, String> {
    init_database(app)?;

    let conn = open_database(app)?;
    let limit = limit.clamp(1, 2_000);
    let keyword = keyword.to_lowercase();
    let pattern = format!("%{keyword}%");
    let matched_filter = matched_only.map(i64::from);

    let sql = match (keyword.is_empty(), matched_filter.is_some()) {
        (true, false) => {
            "
            SELECT id, address, private_key, rule_label, attempts, matched, matched_parts, matched_rules, created_at
            FROM wallets
            ORDER BY created_at DESC
            LIMIT ?1
            "
        }
        (true, true) => {
            "
            SELECT id, address, private_key, rule_label, attempts, matched, matched_parts, matched_rules, created_at
            FROM wallets
            WHERE matched = ?2
            ORDER BY created_at DESC
            LIMIT ?1
            "
        }
        (false, false) => {
            "
            SELECT id, address, private_key, rule_label, attempts, matched, matched_parts, matched_rules, created_at
            FROM wallets
            WHERE lower(address) LIKE ?2
               OR lower(private_key) LIKE ?2
               OR lower(rule_label) LIKE ?2
            ORDER BY created_at DESC
            LIMIT ?1
            "
        }
        (false, true) => {
            "
            SELECT id, address, private_key, rule_label, attempts, matched, matched_parts, matched_rules, created_at
            FROM wallets
            WHERE matched = ?2
              AND (
                lower(address) LIKE ?3
                OR lower(private_key) LIKE ?3
                OR lower(rule_label) LIKE ?3
              )
            ORDER BY created_at DESC
            LIMIT ?1
            "
        }
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|err| format!("准备 SQLite 查询失败：{err}"))?;

    let rows = match (keyword.is_empty(), matched_filter) {
        (true, None) => stmt
            .query_map(params![limit], map_wallet_row)
            .map_err(|err| format!("读取钱包记录失败：{err}"))?,
        (true, Some(matched_value)) => stmt
            .query_map(params![limit, matched_value], map_wallet_row)
            .map_err(|err| format!("读取钱包记录失败：{err}"))?,
        (false, None) => stmt
            .query_map(params![limit, pattern], map_wallet_row)
            .map_err(|err| format!("读取钱包记录失败：{err}"))?,
        (false, Some(matched_value)) => stmt
            .query_map(params![limit, matched_value, pattern], map_wallet_row)
            .map_err(|err| format!("读取钱包记录失败：{err}"))?,
    };

    collect_wallet_rows(rows)
}

fn map_wallet_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WalletItem> {
    let matched_value: i64 = row.get(5)?;
    let matched_parts_text: String = row.get(6)?;
    let matched_rules_text: String = row.get(7)?;

    Ok(WalletItem {
        id: row.get(0)?,
        address: row.get(1)?,
        private_key: row.get(2)?,
        rule_label: row.get(3)?,
        attempts: row.get(4)?,
        matched: matched_value == 1,
        matched_parts: parse_json_list(&matched_parts_text),
        matched_rules: parse_json_list(&matched_rules_text),
        created_at: row.get(8)?,
    })
}

fn collect_wallet_rows<T>(rows: T) -> Result<Vec<WalletItem>, String>
where
    T: IntoIterator<Item = rusqlite::Result<WalletItem>>,
{
    let mut wallets = Vec::new();

    for item in rows {
        wallets.push(item.map_err(|err| format!("解析钱包记录失败：{err}"))?);
    }

    Ok(wallets)
}

fn json_list(list: &[String]) -> Result<String, String> {
    serde_json::to_string(list).map_err(|err| format!("序列化命中规则失败：{err}"))
}

fn parse_json_list(value: &str) -> Vec<String> {
    serde_json::from_str(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shape_aabb() {
        assert!(shape_matches("1122", "aabb"));
        assert!(shape_matches("9933", "aabb"));
        assert!(shape_matches("5577", "aabb"));
        assert!(!shape_matches("1111", "aabb"));
        assert!(!shape_matches("1221", "aabb"));
        assert!(!shape_matches("1234", "aabb"));
    }

    #[test]
    fn test_shape_aaaa() {
        assert!(shape_matches("1111", "aaaa"));
        assert!(shape_matches("AAAA", "aaaa"));
        assert!(!shape_matches("1112", "aaaa"));
    }

    #[test]
    fn test_shape_aaa() {
        assert!(shape_matches("111", "aaa"));
        assert!(shape_matches("ZZZ", "aaa"));
        assert!(!shape_matches("112", "aaa"));
    }

    #[test]
    fn test_shape_abab() {
        assert!(shape_matches("1212", "abab"));
        assert!(shape_matches("3131", "abab"));
        assert!(!shape_matches("1111", "abab"));
        assert!(!shape_matches("1234", "abab"));
    }

    #[test]
    fn test_shape_abcd() {
        assert!(shape_matches("1234", "abcd"));
        assert!(shape_matches("9876", "abcd"));
        assert!(!shape_matches("1132", "abcd"));
    }

    #[test]
    fn test_shape_base58_chars() {
        assert!(shape_matches("AABB", "aabb"));
        assert!(shape_matches("xxyy", "aabb"));
    }
}
