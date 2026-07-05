use anyhow::Result;
use serde_json::to_vec;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tracing::info;

use crate::api::{RequestBody, ResponseBody};
use crate::session::SessionStore;
use crate::solve::{solve_ephemeral, solve_get, SolveOptions};

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub stealth: bool,
    pub proxy: Option<String>,
    pub user_agent: Option<String>,
    pub max_requests_per_session: u32,
}

pub struct ServerState {
    sessions: SessionStore,
    stealth: bool,
    proxy: Option<String>,
    user_agent: Option<String>,
}

impl ServerState {
    fn new(cfg: &ServerConfig) -> Self {
        Self {
            sessions: SessionStore::new(cfg.max_requests_per_session),
            stealth: cfg.stealth,
            proxy: cfg.proxy.clone(),
            user_agent: cfg.user_agent.clone(),
        }
    }
}

/// Apply solver-friendly defaults when not already configured by the operator.
pub fn apply_solver_env_defaults() {
    if std::env::var("OBSCURA_SCRIPT_DEADLINE_MS").is_err() {
        std::env::set_var("OBSCURA_SCRIPT_DEADLINE_MS", "120000");
    }
    if std::env::var("OBSCURA_NAV_TIMEOUT_MS").is_err() {
        std::env::set_var("OBSCURA_NAV_TIMEOUT_MS", "130000");
    }
    if std::env::var("OBSCURA_FETCH_TIMEOUT_MS").is_err() {
        std::env::set_var("OBSCURA_FETCH_TIMEOUT_MS", "120000");
    }
}

pub async fn run(cfg: ServerConfig) -> Result<()> {
    apply_solver_env_defaults();

    let addr: std::net::SocketAddr = format!("{}:{}", cfg.host, cfg.port).parse()?;
    let listener = TcpListener::bind(&addr).await?;
    info!(
        "ObscuraSolverr listening on http://{}:{}/v1 (FlareSolverr-compatible API)",
        cfg.host, cfg.port
    );

    let mut state = ServerState::new(&cfg);

    loop {
        let (stream, peer) = listener.accept().await?;
        tracing::debug!("solverr connection from {peer}");
        if let Err(e) = handle_connection(stream, &mut state).await {
            tracing::debug!("solverr connection closed: {e}");
        }
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    state: &mut ServerState,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    loop {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).await? == 0 {
            break;
        }
        let request_line = request_line.trim();
        if request_line.is_empty() {
            break;
        }

        let parts: Vec<&str> = request_line.splitn(3, ' ').collect();
        if parts.len() < 3 {
            break;
        }
        let method = parts[0];
        let path = parts[1];

        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            let trimmed = line.trim_end_matches("\r\n").trim_end_matches('\n');
            if trimmed.is_empty() {
                break;
            }
            let lower = trimmed.to_lowercase();
            if let Some(v) = lower.strip_prefix("content-length:") {
                content_length = v.trim().parse().ok();
            }
        }

        let body = read_body(&mut reader, content_length).await?;
        let response = if method == "POST" && (path == "/" || path == "/v1") {
            dispatch(state, &body).await
        } else if method == "GET" && path == "/health" {
            ResponseBody::ok("ObscuraSolverr is ready", None, None)
        } else {
            write_http(&mut writer, 404, b"Not Found").await?;
            continue;
        };

        let json = to_vec(&response)?;
        write_http(&mut writer, 200, &json).await?;
    }

    Ok(())
}

async fn read_body(reader: &mut (impl AsyncBufReadExt + Unpin), len: Option<usize>) -> Result<Vec<u8>> {
    let Some(n) = len else {
        return Ok(Vec::new());
    };
    if n > MAX_BODY_BYTES {
        anyhow::bail!("request body too large");
    }
    let mut buf = vec![0u8; n];
    reader.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_http(writer: &mut tokio::net::tcp::OwnedWriteHalf, status: u16, body: &[u8]) -> Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(body).await?;
    writer.shutdown().await?;
    Ok(())
}

async fn dispatch(state: &mut ServerState, body: &[u8]) -> ResponseBody {
    let req: RequestBody = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return ResponseBody::err(format!("Invalid JSON: {e}")),
    };

    match req.cmd.as_str() {
        "sessions.create" => {
            let id = state.sessions.create(
                state.proxy.clone(),
                state.stealth,
                state.user_agent.clone(),
            );
            ResponseBody::ok("Session created", Some(id), None)
        }
        "sessions.destroy" => {
            let Some(session_id) = req.session else {
                return ResponseBody::err("session id required");
            };
            if state.sessions.destroy(&session_id) {
                ResponseBody::ok("Session destroyed", Some(session_id), None)
            } else {
                ResponseBody::err(format!("Session not found: {session_id}"))
            }
        }
        "sessions.list" => {
            let sessions = state.sessions.list();
            ResponseBody::ok(format!("{sessions:?}"), None, None)
        }
        "request.get" => handle_request_get(state, req).await,
        "request.post" => ResponseBody::err("request.post is not implemented yet"),
        other => ResponseBody::err(format!("Unknown command: {other}")),
    }
}

async fn handle_request_get(state: &mut ServerState, req: RequestBody) -> ResponseBody {
    let Some(url) = req.url.filter(|u| !u.trim().is_empty()) else {
        return ResponseBody::err("url is required");
    };
    let max_timeout = req.max_timeout.unwrap_or(120_000).max(1_000).min(300_000);

    if let Some(session_id) = req.session.clone() {
        if let Err(e) = state.sessions.touch(&session_id) {
            return ResponseBody::err(e);
        }
        let session = match state.sessions.get_mut(&session_id) {
            Some(s) => s,
            None => return ResponseBody::err(format!("Session not found: {session_id}")),
        };
        match solve_get(&mut session.page, &session.context, &url, max_timeout).await {
            Ok(result) => ResponseBody::ok(
                result.message,
                Some(session_id),
                Some(result.solution),
            ),
            Err(e) => ResponseBody::err(e.to_string()),
        }
    } else {
        match solve_ephemeral(SolveOptions {
            url,
            max_timeout_ms: max_timeout,
            proxy: state.proxy.clone(),
            stealth: state.stealth,
            user_agent: state.user_agent.clone(),
        })
        .await
        {
            Ok(result) => ResponseBody::ok(result.message, None, Some(result.solution)),
            Err(e) => ResponseBody::err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_env_defaults() {
        std::env::remove_var("OBSCURA_SCRIPT_DEADLINE_MS");
        apply_solver_env_defaults();
        assert_eq!(
            std::env::var("OBSCURA_SCRIPT_DEADLINE_MS").unwrap(),
            "120000"
        );
    }
}
