use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    sync::Arc,
    time::SystemTime,
};
use tokio::{fs, net::TcpListener, sync::Mutex};

type Cache = Arc<Mutex<HashMap<String, String>>>;

#[derive(Deserialize, Serialize)]
struct CacheMeta {
    url: String,
    status: u16,
    last_mod: Option<String>,
    etag: Option<String>,
    headers: Vec<(String, String)>,
}

#[derive(Clone)]
struct AppState {
    client: reqwest::Client,
    cache: Cache,
    blacklist: Vec<String>,
}

const CACHE_DIR: &str = "./cache";
const LOG_FILE: &str = "output.log";
const BL_FILE: &str = "blacklist.txt";

async fn load_blacklist(path: &str) -> Vec<String> {
    fs::read_to_string(path)
        .await
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn in_blacklist(url: &str, blacklist: &[String]) -> bool {
    blacklist.iter().any(|entry| url.contains(entry.as_str()))
}

fn meta_path(key: &str) -> PathBuf {
    PathBuf::from(CACHE_DIR).join(format!("{}.meta.json", key))
}

fn body_path(key: &str) -> PathBuf {
    PathBuf::from(CACHE_DIR).join(format!("{}.body", key))
}

async fn read_cache(key: &str) -> Option<(CacheMeta, Bytes)> {
    let meta = fs::read_to_string(meta_path(key)).await.ok()?;
    let cache_meta: CacheMeta = serde_json::from_str(&meta).ok()?;
    let body = fs::read(body_path(key)).await.ok()?;
    Some((cache_meta, Bytes::from(body)))
}

async fn write_cache(key: &str, meta: &CacheMeta, body: &Bytes) {
    let meta_json = match serde_json::to_string_pretty(meta) {
        Ok(j) => j,
        Err(_) => return,
    };
    fs::write(meta_path(key), meta_json).await.ok();
    fs::write(body_path(key), body.as_ref()).await.ok();
}

async fn load_cache_index() -> Cache {
    let mut cache = HashMap::new();
    fs::create_dir_all(CACHE_DIR).await.ok();

    let mut dir = match fs::read_dir(CACHE_DIR).await {
        Ok(d) => d,
        Err(_) => return Arc::new(Mutex::new(cache)),
    };

    while let Ok(Some(entry)) = dir.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(data) = fs::read_to_string(&path).await {
                if let Ok(meta) = serde_json::from_str::<CacheMeta>(&data) {
                    let key = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .trim_end_matches(".meta")
                        .to_string();
                    cache.insert(meta.url.clone(), key);
                }
            }
        }
    }
    Arc::new(Mutex::new(cache))
}

async fn read_and_cache(
    state: &AppState,
    url: &str,
    key: &str,
    rsp: reqwest::Response,
) -> Result<Response, (StatusCode, String)> {
    let status = rsp.status();

    let last_mod = rsp
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let etag = rsp
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let rsp_headers: Vec<(String, String)> = rsp
        .headers()
        .iter()
        .map(|(n, v)| (n.to_string(), v.to_str().unwrap_or_default().to_string()))
        .collect();
    let body = rsp.bytes().await.unwrap_or_default();
    if status.is_success() && (last_mod.is_some() || etag.is_some()) {
        let meta = CacheMeta {
            url: url.to_string(),
            status: status.as_u16(),
            last_mod,
            etag,
            headers: rsp_headers.clone(),
        };
        write_cache(key, &meta, &body).await;
        state
            .cache
            .lock()
            .await
            .insert(url.to_string(), key.to_string());
    }
    let mut response = Response::builder().status(status);
    for (name, value) in &rsp_headers {
        response = response.header(name.as_str(), value.as_str());
    }
    response
        .body(Body::from(body))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn cached_get(
    state: &AppState,
    url: &str,
    headers: &HeaderMap,
) -> Result<Response, (StatusCode, String)> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let key = format!("{:x}", hasher.finish());
    let cache_key = state.cache.lock().await.get(url).cloned();
    if let Some(ref k) = cache_key {
        if let Some((meta, body)) = read_cache(k).await {
            let mut headers_new = headers.clone();
            if let Some(ref last_mod) = meta.last_mod {
                headers_new.insert(
                    reqwest::header::IF_MODIFIED_SINCE,
                    last_mod.parse().unwrap(),
                );
            }
            if let Some(ref etag) = meta.etag {
                headers_new.insert(reqwest::header::IF_NONE_MATCH, etag.parse().unwrap());
            }
            let rsp = state
                .client
                .get(url)
                .headers(headers_new)
                .send()
                .await
                .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;
            if rsp.status() == StatusCode::NOT_MODIFIED {
                info!("Return response from cache for {}", url);
                return build_rsp_from_cache(&meta, body);
            }
            info!("Read response and update cache for {}", url);
            return read_and_cache(state, url, &key, rsp).await;
        }
    }
    let rsp = state
        .client
        .get(url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;
    return read_and_cache(state, url, &key, rsp).await;
}

fn build_url(raw: &str) -> Option<String> {
    let s = raw.trim_start_matches('/');
    if s.is_empty() {
        return None;
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        Some(s.to_string())
    } else {
        Some(format!("https://{}", s))
    }
}

fn build_rsp_from_cache(meta: &CacheMeta, body: Bytes) -> Result<Response, (StatusCode, String)> {
    let mut response = Response::builder().status(meta.status);
    for (name, value) in &meta.headers {
        response = response.header(name.as_str(), value.as_str());
    }
    response
        .body(Body::from(body))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn proxy_handler(State(state): State<AppState>, req: Request) -> impl IntoResponse {
    let uri = req.uri().clone();
    let method = req.method().clone();
    let headers = req.headers().clone();
    let Some(url) = build_url(uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/")) else {
        return (StatusCode::BAD_REQUEST, "Invalid uri").into_response();
    };
    let body_bytes = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    if in_blacklist(&url, &state.blacklist) {
        warn!("{} in blacklist", uri);
        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())
            .unwrap()
            .into_response();
    }

    if method == Method::GET {
        match cached_get(&state, &url, &headers).await {
            Ok(rsp) => {
                info!("{} {} -> {}", method, url, rsp.status().as_u16());
                return rsp.into_response();
            }
            Err((status, msg)) => {
                error!("{} {} -> {}", method, url, msg);
                return (status, msg).into_response();
            }
        }
    }
    match forward_req(&state.client, &method, &url, &headers, body_bytes).await {
        Ok(rsp) => {
            info!("{} {} -> {}", method, url, rsp.status().as_u16());
            rsp.into_response()
        }
        Err((status, msg)) => {
            error!("{} {} -> {}", method, url, msg);
            (status, msg).into_response()
        }
    }
}

async fn forward_req(
    client: &reqwest::Client,
    method: &Method,
    url: &str,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, (StatusCode, String)> {
    let rsp = client
        .request(method.clone(), url)
        .headers(headers.clone())
        .body(body)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Error: {}", e)))?;
    let mut response = Response::builder().status(rsp.status());
    for (name, value) in rsp.headers() {
        response = response.header(name, value);
    }
    let body_bytes = rsp.bytes().await.unwrap_or_default();
    response
        .body(Body::from(body_bytes))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

fn setup_logger() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339_seconds(SystemTime::now()),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Warn)
        .level_for("proxy_server", log::LevelFilter::Info)
        .chain(std::io::stdout())
        .chain(fern::log_file(LOG_FILE).unwrap())
        .apply()
        .unwrap();
}

#[tokio::main]
async fn main() {
    setup_logger();
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Expected 2 args: <server.exe> server_port");
        std::process::exit(1);
    }
    let port: u16 = args[1].parse().expect("Port shall have u16 format");
    let cache = load_cache_index().await;
    let blacklist = load_blacklist(BL_FILE).await;
    let app: Router = Router::new().fallback(proxy_handler).with_state(AppState {
        client: reqwest::Client::new(),
        cache,
        blacklist,
    });
    let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
