use alice_lol::print_export::{lol_to_3mf, lol_to_fbx, PrintConfig};
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

// ---------------------------------------------------------------------------
// Prompt cache
// ---------------------------------------------------------------------------

struct CacheEntry {
    value: String,
    created: Instant,
}

struct PromptCache {
    entries: DashMap<u64, CacheEntry>,
    hits: AtomicU64,
    misses: AtomicU64,
    max_entries: usize,
    ttl_secs: u64,
}

impl PromptCache {
    fn new(max_entries: usize, ttl_secs: u64) -> Self {
        Self {
            entries: DashMap::new(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            max_entries,
            ttl_secs,
        }
    }

    fn cache_key(prompt: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        prompt.hash(&mut hasher);
        hasher.finish()
    }

    fn get(&self, prompt: &str) -> Option<String> {
        let key = Self::cache_key(prompt);
        if let Some(entry) = self.entries.get(&key) {
            if entry.created.elapsed().as_secs() < self.ttl_secs {
                self.hits.fetch_add(1, Ordering::Relaxed);
                return Some(entry.value.clone());
            }
            drop(entry);
            self.entries.remove(&key);
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    fn put(&self, prompt: &str, value: String) {
        if self.entries.len() >= self.max_entries {
            self.evict_expired();
        }
        if self.entries.len() >= self.max_entries {
            if let Some(oldest_key) = self.entries.iter()
                .min_by_key(|e| e.value().created)
                .map(|e| *e.key())
            {
                self.entries.remove(&oldest_key);
            }
        }
        let key = Self::cache_key(prompt);
        self.entries.insert(key, CacheEntry { value, created: Instant::now() });
    }

    fn evict_expired(&self) {
        let ttl = self.ttl_secs;
        self.entries.retain(|_, v| v.created.elapsed().as_secs() < ttl);
    }

    fn stats(&self) -> (u64, u64, usize) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
            self.entries.len(),
        )
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct AppState {
    jwt_secret: String,
    supabase_url: String,
    supabase_service_key: String,
    rate_limiters: DashMap<String, TokenBucket>,
    start_time: Instant,
    llm_endpoint: String,
    system_prompt: String,
    output_dir: String,
    cache: PromptCache,
}

struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(max: f64, rate: f64) -> Self {
        Self { tokens: max, max_tokens: max, refill_rate: rate, last_refill: Instant::now() }
    }
    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
        if self.tokens >= 1.0 { self.tokens -= 1.0; true } else { false }
    }
}

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct Health {
    status: String,
    version: String,
    uptime_secs: u64,
    llm_endpoint: String,
    printers: Vec<PrinterInfo>,
    cache: CacheStats,
}

#[derive(Serialize)]
struct CacheStats {
    hits: u64,
    misses: u64,
    entries: usize,
    hit_rate: String,
}

#[derive(Serialize)]
struct Err {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

#[derive(Serialize)]
struct LicenseInfo { license: String, notice: String }

#[derive(Deserialize, Serialize, Clone)]
struct Claims {
    sub: String,
    email: Option<String>,
    role: Option<String>,
    exp: usize,
    #[serde(default)]
    plan: Option<String>,
}

// ---------------------------------------------------------------------------
// Core Engine types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[allow(dead_code)]
struct GenerateRequest {
    prompt: String,
    #[serde(default = "default_quality")]
    quality: String,
    #[serde(default = "default_format")]
    format: String,
}

#[derive(Deserialize)]
struct DirectLolRequest {
    lol_source: String,
    #[serde(default = "default_quality")]
    quality: String,
    #[serde(default = "default_format")]
    format: String,
}

fn default_quality() -> String { "high".into() }
fn default_format() -> String { "3mf".into() }

fn can_download(plan: &str) -> bool {
    matches!(plan, "General" | "Pro" | "Enterprise")
}

fn quality_to_config(quality: &str) -> PrintConfig {
    match quality {
        "preview" => PrintConfig::preview(),
        "ultra" => PrintConfig::high_quality(),
        _ => PrintConfig::default(),
    }
}

#[derive(Serialize)]
struct GenerateResponse {
    job_id: String,
    status: String,
    lol_source: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
struct PrinterInfo {
    id: String,
    name: String,
    build_volume_mm: [f32; 3],
    max_with_margin_mm: [f32; 3],
}

fn printers() -> Vec<PrinterInfo> {
    vec![
        PrinterInfo {
            id: "bambu_h2d".into(),
            name: "Bambu Lab H2D (single nozzle)".into(),
            build_volume_mm: [325.0, 320.0, 320.0],
            max_with_margin_mm: [315.0, 310.0, 315.0],
        },
        PrinterInfo {
            id: "bambu_h2d_dual".into(),
            name: "Bambu Lab H2D (dual nozzle)".into(),
            build_volume_mm: [300.0, 320.0, 325.0],
            max_with_margin_mm: [290.0, 310.0, 315.0],
        },
    ]
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_gateway=info,tower_http=info".into()),
        )
        .init();
    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.into());
    let output_dir = env("OUTPUT_DIR", "/tmp/3dvbgaran");
    std::fs::create_dir_all(&output_dir).expect("failed to create output dir");
    let cache_max: usize = env("CACHE_MAX_ENTRIES", "1000").parse().unwrap_or(1000);
    let cache_ttl: u64 = env("CACHE_TTL_SECS", "3600").parse().unwrap_or(3600);
    let state = Arc::new(AppState {
        jwt_secret: env("JWT_SECRET", "dev-secret-change-me"),
        supabase_url: env("SUPABASE_URL", ""),
        supabase_service_key: env("SUPABASE_SERVICE_ROLE_KEY", ""),
        rate_limiters: DashMap::new(),
        start_time: Instant::now(),
        llm_endpoint: env("LLM_ENDPOINT", "http://localhost:8000"),
        system_prompt: load_system_prompt(),
        output_dir,
        cache: PromptCache::new(cache_max, cache_ttl),
    });
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    let frontend_url = env("FRONTEND_URL", "http://127.0.0.1:3000");

    let public = Router::new()
        .route("/health", get(health))
        .route("/license", get(license_handler));

    let api = Router::new()
        .route("/api/v1/generate", post(generate))
        .route("/api/v1/preview", post(generate_preview))
        .route("/api/v1/generate-lol", post(generate_from_lol))
        .layer(middleware::from_fn_with_state(state.clone(), auth_mw))
        .layer(middleware::from_fn_with_state(state.clone(), rate_mw));

    let admin = Router::new()
        .route("/api/v1/admin/stats", get(admin_stats))
        .route("/api/v1/admin/users", get(admin_users))
        .route("/api/v1/admin/users/{id}", axum::routing::patch(admin_update_user))
        .route("/api/v1/admin/generations", get(admin_generations))
        .route("/api/v1/admin/projects", get(admin_projects))
        .route("/api/v1/admin/projects/{id}", axum::routing::patch(admin_update_project))
        .route("/api/v1/admin/revenue", get(admin_revenue))
        .layer(middleware::from_fn_with_state(state.clone(), admin_mw))
        .layer(middleware::from_fn_with_state(state.clone(), auth_mw));

    let frontend_proxy = Router::new()
        .fallback(move |req: Request| proxy_frontend(frontend_url.clone(), req));

    let app = Router::new()
        .merge(public)
        .merge(api)
        .merge(admin)
        .merge(frontend_proxy)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("3dvbgaran on {addr}");
    axum::serve(listener, app).await.unwrap();
}

// ---------------------------------------------------------------------------
// Public handlers
// ---------------------------------------------------------------------------

async fn health(State(s): State<Arc<AppState>>) -> Json<Health> {
    let (hits, misses, entries) = s.cache.stats();
    let total = hits + misses;
    let hit_rate = if total > 0 { format!("{:.1}%", hits as f64 / total as f64 * 100.0) } else { "N/A".into() };
    Json(Health {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        uptime_secs: s.start_time.elapsed().as_secs(),
        llm_endpoint: s.llm_endpoint.clone(),
        printers: printers(),
        cache: CacheStats { hits, misses, entries, hit_rate },
    })
}

async fn license_handler() -> (HeaderMap, Json<LicenseInfo>) {
    let mut h = HeaderMap::new();
    h.insert("X-License", "MIT".parse().unwrap());
    (h, Json(LicenseInfo {
        license: "MIT".into(),
        notice: "3dvbgaran".into(),
    }))
}

// ---------------------------------------------------------------------------
// Core Engine handlers (integrated)
// ---------------------------------------------------------------------------

async fn generate(
    State(s): State<Arc<AppState>>,
    req: Request,
) -> Result<Response, (StatusCode, Json<Err>)> {
    let claims = req.extensions().get::<Claims>().cloned();
    let plan = claims.as_ref().and_then(|c| c.plan.as_deref()).unwrap_or("Free");

    let body = axum::body::to_bytes(req.into_body(), 1024 * 1024)
        .await
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(Err { error: "Bad request body".into(), details: None })))?;
    let gen_req: GenerateRequest = serde_json::from_slice(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(Err { error: format!("Invalid JSON: {e}"), details: None })))?;

    let job_id = uuid::Uuid::new_v4().to_string();
    tracing::info!(job_id = %job_id, prompt = %gen_req.prompt, "generate");

    let lol_source = cached_llm(&s, &gen_req.prompt).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(Err { error: format!("LLM error: {e}"), details: None })))?;

    if !can_download(plan) {
        return Ok(Json(GenerateResponse {
            job_id,
            status: "preview".into(),
            lol_source: Some(lol_source),
            error: None,
        }).into_response());
    }

    build_mesh_response(&s.output_dir, &job_id, &lol_source, &gen_req.quality, &gen_req.format).await
}

async fn generate_preview(
    State(s): State<Arc<AppState>>,
    Json(req): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, (StatusCode, Json<Err>)> {
    let job_id = uuid::Uuid::new_v4().to_string();

    let lol_source = cached_llm(&s, &req.prompt).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(Err { error: format!("LLM error: {e}"), details: None })))?;

    Ok(Json(GenerateResponse {
        job_id,
        status: "preview".into(),
        lol_source: Some(lol_source),
        error: None,
    }))
}

async fn cached_llm(state: &AppState, prompt: &str) -> Result<String, String> {
    if let Some(cached) = state.cache.get(prompt) {
        tracing::info!(prompt = %prompt, "cache hit");
        return Ok(cached);
    }
    let result = call_llm(&state.llm_endpoint, &state.system_prompt, prompt).await?;
    state.cache.put(prompt, result.clone());
    Ok(result)
}

async fn generate_from_lol(
    State(s): State<Arc<AppState>>,
    req: Request,
) -> Result<Response, (StatusCode, Json<Err>)> {
    let claims = req.extensions().get::<Claims>().cloned();
    let plan = claims.as_ref().and_then(|c| c.plan.as_deref()).unwrap_or("Free");

    let body = axum::body::to_bytes(req.into_body(), 1024 * 1024)
        .await
        .map_err(|_| (StatusCode::BAD_REQUEST, Json(Err { error: "Bad request body".into(), details: None })))?;
    let lol_req: DirectLolRequest = serde_json::from_slice(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(Err { error: format!("Invalid JSON: {e}"), details: None })))?;

    let job_id = uuid::Uuid::new_v4().to_string();
    tracing::info!(job_id = %job_id, "generate_from_lol");

    if !can_download(plan) {
        return Ok(Json(GenerateResponse {
            job_id,
            status: "preview".into(),
            lol_source: Some(lol_req.lol_source),
            error: None,
        }).into_response());
    }

    build_mesh_response(&s.output_dir, &job_id, &lol_req.lol_source, &lol_req.quality, &lol_req.format).await
}

async fn build_mesh_response(
    output_dir: &str, job_id: &str, lol_source: &str, quality: &str, format: &str,
) -> Result<Response, (StatusCode, Json<Err>)> {
    let config = quality_to_config(quality);
    let ext = if format == "fbx" { "fbx" } else { "3mf" };
    let out_path = format!("{output_dir}/{job_id}.{ext}");

    let lol_clone = lol_source.to_string();
    let path_clone = out_path.clone();
    let use_fbx = ext == "fbx";
    let stats = tokio::task::spawn_blocking(move || {
        if use_fbx { lol_to_fbx(&lol_clone, &path_clone, &config) }
        else { lol_to_3mf(&lol_clone, &path_clone, &config) }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(Err { error: format!("task join: {e}"), details: None })))?
    .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, Json(Err { error: format!("LOL pipeline: {e}"), details: None })))?;

    let bytes = tokio::fs::read(&out_path).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(Err { error: format!("read file: {e}"), details: None }))
    })?;
    let _ = tokio::fs::remove_file(&out_path).await;

    let job_id_owned = job_id.to_string();
    let lol_owned = lol_source.to_string();

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{job_id_owned}.{ext}\""))
        .header("X-Job-Id", &job_id_owned)
        .header("X-Triangle-Count", stats.triangle_count.to_string())
        .header("X-Vertex-Count", stats.vertex_count.to_string())
        .header("X-LOL-Source", lol_owned)
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

use axum::response::IntoResponse;
impl IntoResponse for GenerateResponse {
    fn into_response(self) -> Response { Json(self).into_response() }
}

// ---------------------------------------------------------------------------
// Auth middleware
// ---------------------------------------------------------------------------

async fn validate_api_key(state: &AppState, key: &str) -> Option<Claims> {
    if state.supabase_url.is_empty() || state.supabase_service_key.is_empty() {
        return Some(Claims {
            sub: "api-key-user".into(), email: None,
            role: Some("api".into()), exp: usize::MAX,
            plan: Some("Free".into()),
        });
    }

    let client = reqwest::Client::new();
    let url = format!("{}/rest/v1/profiles?api_key=eq.{}&select=id,plan", state.supabase_url, key);

    #[derive(Deserialize)]
    struct Profile { id: String, plan: Option<String> }

    let resp = client
        .get(&url)
        .header("apikey", &state.supabase_service_key)
        .header("Authorization", format!("Bearer {}", state.supabase_service_key))
        .send().await.ok()?;

    let profiles: Vec<Profile> = resp.json().await.ok()?;
    let profile = profiles.first()?;

    Some(Claims {
        sub: profile.id.clone(), email: None,
        role: Some("api".into()), exp: usize::MAX,
        plan: Some(profile.plan.clone().unwrap_or_else(|| "Free".into())),
    })
}

async fn auth_mw(
    State(s): State<Arc<AppState>>, mut req: Request, next: Next,
) -> Result<Response, (StatusCode, Json<Err>)> {
    let auth = req.headers().get("Authorization").and_then(|h| h.to_str().ok()).map(|s| s.to_string());
    let api_key = req.headers().get("X-API-Key").and_then(|h| h.to_str().ok()).map(|s| s.to_string());

    if let Some(a) = &auth {
        if let Some(token) = a.strip_prefix("Bearer ") {
            let mut val = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
            val.validate_aud = false;
            match jsonwebtoken::decode::<Claims>(
                token, &jsonwebtoken::DecodingKey::from_secret(s.jwt_secret.as_bytes()), &val,
            ) {
                Ok(data) => { req.extensions_mut().insert(data.claims); return Ok(next.run(req).await); }
                Err(e) => return Err((StatusCode::UNAUTHORIZED, Json(Err { error: "Invalid token".into(), details: Some(e.to_string()) }))),
            }
        }
    }

    if let Some(key) = api_key {
        if let Some(claims) = validate_api_key(&s, &key).await {
            req.extensions_mut().insert(claims);
            return Ok(next.run(req).await);
        }
        return Err((StatusCode::UNAUTHORIZED, Json(Err { error: "Invalid API key".into(), details: None })));
    }

    Err((StatusCode::UNAUTHORIZED, Json(Err { error: "Auth required".into(), details: Some("Provide Bearer token or X-API-Key".into()) })))
}

// ---------------------------------------------------------------------------
// Rate limit middleware
// ---------------------------------------------------------------------------

async fn rate_mw(
    State(s): State<Arc<AppState>>, req: Request, next: Next,
) -> Result<Response, (StatusCode, Json<Err>)> {
    let claims = req.extensions().get::<Claims>().cloned();
    let uid = claims.as_ref().map(|c| c.sub.clone()).unwrap_or_else(|| "anon".into());
    let plan = claims.as_ref().and_then(|c| c.plan.as_deref()).unwrap_or("Free");

    let max_tokens = match plan {
        "Enterprise" => 100_000.0,
        "Pro" => 10_000.0,
        "General" => 1_000.0,
        _ => 100.0,
    };

    let ok = {
        let mut e = s.rate_limiters.entry(uid.clone()).or_insert_with(|| TokenBucket::new(max_tokens, max_tokens / 3600.0));
        if (e.max_tokens - max_tokens).abs() > 1.0 {
            *e = TokenBucket::new(max_tokens, max_tokens / 3600.0);
        }
        e.try_consume()
    };
    if !ok {
        return Err((StatusCode::TOO_MANY_REQUESTS, Json(Err { error: "Rate limit exceeded".into(), details: None })));
    }

    let state = s.clone();
    let method = req.method().to_string();
    let endpoint = req.uri().path().to_string();
    let uid_clone = uid.clone();
    let start = Instant::now();

    let resp = next.run(req).await;

    let status_code = resp.status().as_u16() as i32;
    let response_time_ms = start.elapsed().as_secs_f64() * 1000.0;

    tokio::spawn(async move {
        record_usage(&state, &uid_clone, &endpoint, &method, status_code, response_time_ms).await;
    });

    Ok(resp)
}

async fn record_usage(
    state: &AppState, user_id: &str, endpoint: &str, method: &str,
    status_code: i32, response_time_ms: f64,
) {
    if state.supabase_url.is_empty() || state.supabase_service_key.is_empty() { return; }
    if user_id.len() != 36 { return; }

    let client = reqwest::Client::new();
    let url = format!("{}/rest/v1/api_usage", state.supabase_url);
    let body = serde_json::json!({
        "user_id": user_id, "endpoint": endpoint, "method": method,
        "status_code": status_code, "response_time_ms": response_time_ms,
    });
    let _ = client.post(&url)
        .header("apikey", &state.supabase_service_key)
        .header("Authorization", format!("Bearer {}", state.supabase_service_key))
        .header("Content-Type", "application/json")
        .json(&body).send().await;
}

// ---------------------------------------------------------------------------
// Frontend proxy
// ---------------------------------------------------------------------------

async fn proxy_frontend(frontend_url: String, req: Request) -> Response {
    // Don't follow redirects — pass them through to the client
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let path = req.uri().path_and_query().map(|pq| pq.to_string()).unwrap_or_else(|| "/".into());
    let method = req.method().clone();
    let hdrs = req.headers().clone();
    let body = axum::body::to_bytes(req.into_body(), 10 * 1024 * 1024).await.unwrap_or_default();

    let target = format!("{frontend_url}{path}");
    tracing::debug!(target = %target, "proxy_frontend");

    let mut r = client.request(method, &target);
    for (k, v) in hdrs.iter() {
        if k != "host" && k != "transfer-encoding" { r = r.header(k, v); }
    }

    match r.body(body).send().await {
        Ok(resp) => {
            let st = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let rh = resp.headers().clone();
            let rb = resp.bytes().await.unwrap_or_default();
            let mut b = Response::builder().status(st);
            for (k, v) in rh.iter() {
                // Rewrite Location header to remove internal URL
                if k == "location" {
                    if let Ok(loc) = v.to_str() {
                        let rewritten = loc
                            .replace("http://127.0.0.1:3000", "")
                            .replace("http://localhost:3000", "");
                        b = b.header(k, rewritten);
                        continue;
                    }
                }
                // Skip transfer-encoding — axum handles it
                if k == "transfer-encoding" { continue; }
                b = b.header(k, v);
            }
            b.body(axum::body::Body::from(rb)).unwrap_or_else(|_| {
                Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(axum::body::Body::from("proxy error")).unwrap()
            })
        }
        Err(e) => {
            tracing::error!(error = %e, target = %target, "frontend proxy failed");
            Response::builder().status(StatusCode::BAD_GATEWAY)
                .header("content-type", "text/plain")
                .body(axum::body::Body::from("Frontend unavailable")).unwrap()
        }
    }
}

// ---------------------------------------------------------------------------
// LLM client (OpenAI compatible)
// ---------------------------------------------------------------------------

async fn call_llm(endpoint: &str, system_prompt: &str, user_prompt: &str) -> Result<String, String> {
    #[derive(Serialize)]
    struct Req { model: String, messages: Vec<Msg>, temperature: f32, max_tokens: u32 }
    #[derive(Serialize)]
    struct Msg { role: String, content: String }
    #[derive(Deserialize)]
    struct Resp { choices: Vec<Choice> }
    #[derive(Deserialize)]
    struct Choice { message: ChoiceMsg }
    #[derive(Deserialize)]
    struct ChoiceMsg { content: String }

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{endpoint}/v1/chat/completions"))
        .json(&Req {
            model: std::env::var("LLM_MODEL").unwrap_or_else(|_| "qwen3.5:9b".into()),
            messages: vec![
                Msg { role: "system".into(), content: format!("/no_think\n{system_prompt}") },
                Msg { role: "user".into(), content: user_prompt.into() },
            ],
            temperature: 0.3, max_tokens: 2048,
        })
        .send().await.map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        let st = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("{st}: {body}"));
    }

    let r: Resp = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    r.choices.first().map(|c| extract_lol_block(&c.message.content))
        .ok_or_else(|| "no choices".into())
}

fn extract_lol_block(content: &str) -> String {
    if let Some(start) = content.find("```lol") {
        let after = &content[start + 6..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let Some(start) = content.find("```") {
        let after = &content[start + 3..];
        let after = after.find('\n').map_or(after, |nl| &after[nl + 1..]);
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    content.trim().to_string()
}


fn load_system_prompt() -> String {
    if let Ok(custom) = std::fs::read_to_string("system_prompt.md") { return custom; }
    include_str!("system_prompt.md").to_string()
}

// ---------------------------------------------------------------------------
// Admin middleware
// ---------------------------------------------------------------------------

async fn admin_mw(
    State(s): State<Arc<AppState>>, req: Request, next: Next,
) -> Result<Response, (StatusCode, Json<Err>)> {
    let claims = req.extensions().get::<Claims>().cloned();
    let uid = claims.as_ref().map(|c| c.sub.clone()).unwrap_or_default();

    if s.supabase_url.is_empty() || s.supabase_service_key.is_empty() {
        return Ok(next.run(req).await);
    }

    let client = reqwest::Client::new();
    let url = format!("{}/rest/v1/profiles?id=eq.{}&select=role", s.supabase_url, uid);

    #[derive(Deserialize)]
    struct RoleCheck { role: Option<String> }

    let is_admin = match client.get(&url)
        .header("apikey", &s.supabase_service_key)
        .header("Authorization", format!("Bearer {}", s.supabase_service_key))
        .send().await
    {
        Ok(resp) => resp.json::<Vec<RoleCheck>>().await.ok()
            .and_then(|v| v.first().and_then(|p| p.role.as_deref().map(|r| r == "admin")))
            .unwrap_or(false),
        Err(_) => false,
    };

    if !is_admin {
        return Err((StatusCode::FORBIDDEN, Json(Err { error: "Admin access required".into(), details: None })));
    }
    Ok(next.run(req).await)
}

// ---------------------------------------------------------------------------
// Admin handlers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct AdminStats {
    uptime_secs: u64,
    llm_endpoint: String,
    llm_online: bool,
    total_users: i64,
    total_generations: i64,
    today_generations: i64,
    active_rate_limiters: usize,
    cache_hits: u64,
    cache_misses: u64,
    cache_entries: usize,
}

async fn admin_stats(State(s): State<Arc<AppState>>) -> Json<AdminStats> {
    let client = reqwest::Client::new();
    let total_users = supabase_count(&client, &s, "profiles", "").await;
    let total_generations = supabase_count(&client, &s, "generations", "").await;
    let today = chrono_today();
    let today_generations = supabase_count(&client, &s, "generations", &format!("&created_at=gte.{today}T00:00:00Z")).await;

    let llm_online = client.get(format!("{}/health", s.llm_endpoint))
        .timeout(std::time::Duration::from_secs(3))
        .send().await.map(|r| r.status().is_success()).unwrap_or(false);

    let (hits, misses, entries) = s.cache.stats();

    Json(AdminStats {
        uptime_secs: s.start_time.elapsed().as_secs(),
        llm_endpoint: s.llm_endpoint.clone(),
        llm_online, total_users, total_generations, today_generations,
        active_rate_limiters: s.rate_limiters.len(),
        cache_hits: hits, cache_misses: misses, cache_entries: entries,
    })
}

async fn admin_users(State(s): State<Arc<AppState>>) -> Result<Response, (StatusCode, Json<Err>)> {
    supabase_get(&s, "profiles?select=id,email,full_name,plan,role,banned,created_at&order=created_at.desc&limit=200").await
}

async fn admin_update_user(
    State(s): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, (StatusCode, Json<Err>)> {
    let allowed = ["plan", "role", "banned"];
    let filtered: serde_json::Map<String, serde_json::Value> = body.as_object()
        .map(|o| o.iter().filter(|(k, _)| allowed.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    if filtered.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(Err { error: "No valid fields".into(), details: None })));
    }
    supabase_patch(&s, &format!("profiles?id=eq.{id}"), &serde_json::Value::Object(filtered)).await
}

async fn admin_generations(
    State(s): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Response, (StatusCode, Json<Err>)> {
    let limit = params.get("limit").and_then(|v| v.parse::<u32>().ok()).unwrap_or(50);
    let offset = params.get("offset").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
    let search = params.get("search").cloned().unwrap_or_default();
    let mut query = format!(
        "generations?select=id,user_id,prompt,lol_source,triangle_count,quality,status,error,created_at&order=created_at.desc&limit={limit}&offset={offset}"
    );
    if !search.is_empty() { query.push_str(&format!("&prompt=ilike.*{}*", search)); }
    supabase_get(&s, &query).await
}

async fn admin_projects(State(s): State<Arc<AppState>>) -> Result<Response, (StatusCode, Json<Err>)> {
    supabase_get(&s, "projects?select=id,name,owner_id,is_public,hidden,created_at,updated_at&order=updated_at.desc&limit=200").await
}

async fn admin_update_project(
    State(s): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, (StatusCode, Json<Err>)> {
    let allowed = ["hidden", "is_public"];
    let filtered: serde_json::Map<String, serde_json::Value> = body.as_object()
        .map(|o| o.iter().filter(|(k, _)| allowed.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();
    if filtered.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(Err { error: "No valid fields".into(), details: None })));
    }
    supabase_patch(&s, &format!("projects?id=eq.{id}"), &serde_json::Value::Object(filtered)).await
}

async fn admin_revenue(State(s): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, (StatusCode, Json<Err>)> {
    let client = reqwest::Client::new();
    let general = supabase_count(&client, &s, "profiles", "&plan=eq.General").await;
    let pro = supabase_count(&client, &s, "profiles", "&plan=eq.Pro").await;
    let enterprise = supabase_count(&client, &s, "profiles", "&plan=eq.Enterprise").await;
    let mrr = general * 1500 + pro * 5000;
    Ok(Json(serde_json::json!({
        "subscribers": { "general": general, "pro": pro, "enterprise": enterprise },
        "mrr_jpy": mrr,
        "note": "Enterprise revenue not included (custom pricing)"
    })))
}

// ---------------------------------------------------------------------------
// Supabase admin helpers
// ---------------------------------------------------------------------------

async fn supabase_count(client: &reqwest::Client, s: &AppState, table: &str, filter: &str) -> i64 {
    if s.supabase_url.is_empty() { return 0; }
    client.get(format!("{}/rest/v1/{table}?select=id{filter}", s.supabase_url))
        .header("apikey", &s.supabase_service_key)
        .header("Authorization", format!("Bearer {}", s.supabase_service_key))
        .header("Prefer", "count=exact")
        .header("Range-Unit", "items").header("Range", "0-0")
        .send().await.ok()
        .and_then(|r| r.headers().get("content-range").and_then(|v| v.to_str().ok().map(|s| s.to_string()))
            .map(|cr| cr.split('/').next_back().and_then(|n| n.parse::<i64>().ok()).unwrap_or(0)))
        .unwrap_or(0)
}

async fn supabase_get(s: &AppState, path: &str) -> Result<Response, (StatusCode, Json<Err>)> {
    if s.supabase_url.is_empty() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, Json(Err { error: "Supabase not configured".into(), details: None })));
    }
    let resp = reqwest::Client::new().get(format!("{}/rest/v1/{path}", s.supabase_url))
        .header("apikey", &s.supabase_service_key)
        .header("Authorization", format!("Bearer {}", s.supabase_service_key))
        .send().await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(Err { error: format!("supabase: {e}"), details: None })))?;
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let body = resp.bytes().await.unwrap_or_default();
    Ok(Response::builder().status(status).header("content-type", "application/json")
        .body(axum::body::Body::from(body)).unwrap())
}

async fn supabase_patch(s: &AppState, path: &str, body: &serde_json::Value) -> Result<Response, (StatusCode, Json<Err>)> {
    if s.supabase_url.is_empty() {
        return Err((StatusCode::SERVICE_UNAVAILABLE, Json(Err { error: "Supabase not configured".into(), details: None })));
    }
    let resp = reqwest::Client::new().patch(format!("{}/rest/v1/{path}", s.supabase_url))
        .header("apikey", &s.supabase_service_key)
        .header("Authorization", format!("Bearer {}", s.supabase_service_key))
        .header("Content-Type", "application/json").header("Prefer", "return=representation")
        .json(body).send().await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(Err { error: format!("supabase: {e}"), details: None })))?;
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let rb = resp.bytes().await.unwrap_or_default();
    Ok(Response::builder().status(status).header("content-type", "application/json")
        .body(axum::body::Body::from(rb)).unwrap())
}

fn chrono_today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    let days = secs.div_euclid(86400) + 719468;
    let era = days.div_euclid(146097);
    let doe = days.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_lol_tagged() {
        let input = "Here:\n```lol\nsphere { radius: 10 }\n```\nDone.";
        assert_eq!(extract_lol_block(input), "sphere { radius: 10 }");
    }

    #[test]
    fn extract_lol_generic() {
        let input = "```\nbox3d { size: [10, 20, 30] }\n```";
        assert_eq!(extract_lol_block(input), "box3d { size: [10, 20, 30] }");
    }

    #[test]
    fn extract_lol_raw() {
        let input = "sphere { radius: 5 }";
        assert_eq!(extract_lol_block(input), "sphere { radius: 5 }");
    }

    #[test]
    fn printers_not_empty() {
        assert!(!printers().is_empty());
        assert_eq!(printers()[0].id, "bambu_h2d");
    }

    #[test]
    fn cache_hit_miss() {
        let cache = PromptCache::new(10, 3600);
        assert!(cache.get("hello").is_none());
        let (h, m, _) = cache.stats();
        assert_eq!((h, m), (0, 1));

        cache.put("hello", "world".into());
        assert_eq!(cache.get("hello").unwrap(), "world");
        let (h, m, e) = cache.stats();
        assert_eq!((h, m, e), (1, 1, 1));
    }

    #[test]
    fn cache_eviction() {
        let cache = PromptCache::new(2, 3600);
        cache.put("a", "1".into());
        cache.put("b", "2".into());
        cache.put("c", "3".into());
        assert_eq!(cache.entries.len(), 2);
    }

    #[test]
    fn download_gate() {
        assert!(!can_download("Free"));
        assert!(can_download("General"));
        assert!(can_download("Pro"));
        assert!(can_download("Enterprise"));
    }
}
