use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

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
struct GenerateRequest {
    prompt: String,
    #[serde(default = "default_quality")]
    quality: String,
}

#[derive(Deserialize)]
struct DirectLolRequest {
    lol_source: String,
    #[serde(default = "default_quality")]
    quality: String,
}

fn default_quality() -> String { "high".into() }

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
    let state = Arc::new(AppState {
        jwt_secret: env("JWT_SECRET", "dev-secret-change-me"),
        supabase_url: env("SUPABASE_URL", ""),
        supabase_service_key: env("SUPABASE_SERVICE_ROLE_KEY", ""),
        rate_limiters: DashMap::new(),
        start_time: Instant::now(),
        llm_endpoint: env("LLM_ENDPOINT", "http://localhost:8000"),
        system_prompt: load_system_prompt(),
    });
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    let public = Router::new()
        .route("/health", get(health))
        .route("/license", get(license_handler));

    let api = Router::new()
        .route("/api/v1/generate", post(generate))
        .route("/api/v1/preview", post(generate_preview))
        .route("/api/v1/generate-lol", post(generate_from_lol))
        .layer(middleware::from_fn_with_state(state.clone(), auth_mw))
        .layer(middleware::from_fn_with_state(state.clone(), rate_mw));

    let app = Router::new()
        .merge(public)
        .merge(api)
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
    Json(Health {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        uptime_secs: s.start_time.elapsed().as_secs(),
        llm_endpoint: s.llm_endpoint.clone(),
        printers: printers(),
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
    Json(req): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, (StatusCode, Json<GenerateResponse>)> {
    let job_id = uuid::Uuid::new_v4().to_string();
    tracing::info!(job_id = %job_id, prompt = %req.prompt, "generate");

    let lol_source = call_llm(&s.llm_endpoint, &s.system_prompt, &req.prompt)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(err_resp(&job_id, &format!("LLM error: {e}")))))?;

    // TODO: alice-lol pipeline (lol_to_3mf)
    Ok(Json(GenerateResponse {
        job_id,
        status: "completed".into(),
        lol_source: Some(lol_source),
        error: None,
    }))
}

async fn generate_preview(
    State(s): State<Arc<AppState>>,
    Json(req): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, (StatusCode, Json<GenerateResponse>)> {
    let job_id = uuid::Uuid::new_v4().to_string();

    let lol_source = call_llm(&s.llm_endpoint, &s.system_prompt, &req.prompt)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(err_resp(&job_id, &format!("LLM error: {e}")))))?;

    Ok(Json(GenerateResponse {
        job_id,
        status: "preview".into(),
        lol_source: Some(lol_source),
        error: None,
    }))
}

async fn generate_from_lol(
    State(_s): State<Arc<AppState>>,
    Json(req): Json<DirectLolRequest>,
) -> Result<Json<GenerateResponse>, (StatusCode, Json<GenerateResponse>)> {
    let job_id = uuid::Uuid::new_v4().to_string();

    // TODO: alice-lol pipeline (lol_to_3mf)
    Ok(Json(GenerateResponse {
        job_id,
        status: "completed".into(),
        lol_source: Some(req.lol_source),
        error: None,
    }))
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
            model: "default".into(),
            messages: vec![
                Msg { role: "system".into(), content: system_prompt.into() },
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

fn err_resp(job_id: &str, error: &str) -> GenerateResponse {
    GenerateResponse {
        job_id: job_id.into(), status: "error".into(),
        lol_source: None, error: Some(error.into()),
    }
}

fn load_system_prompt() -> String {
    if let Ok(custom) = std::fs::read_to_string("system_prompt.md") { return custom; }
    include_str!("system_prompt.md").to_string()
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
}
