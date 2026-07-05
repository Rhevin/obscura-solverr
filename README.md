# ObscuraSolverr

FlareSolverr-compatible Cloudflare bypass API, powered by the [Obscura](https://github.com/h4ckf0r0day/obscura) headless browser.

Drop-in replacement on port **8191** — same `POST /v1` contract as FlareSolverr. Works with mangadl-go, Prowlarr, Sonarr, Radarr, and any client that already speaks FlareSolverr.

## Why this fork

| | FlareSolverr | ObscuraSolverr |
|---|--------------|----------------|
| Browser | Chrome/Firefox + Puppeteer (~1.5 GiB) | Obscura + stealth (~30–50 MB per session) |
| API | `POST /v1` on `:8191` | Same |
| License | MIT | Apache 2.0 (Obscura upstream) |

This repo is an Obscura fork with a **`solverr`** subcommand that implements the bypass layer (navigate, wait for challenges, return HTML + cookies).

## Quick start

### Docker (recommended)

```bash
docker compose -f compose.solverr.yaml up -d --build
```

### Binary

```bash
cargo build --release --features stealth --bin obscura

./target/release/obscura solverr --stealth --host 0.0.0.0 --port 8191
```

Stealth is required for TLS fingerprinting and anti-detect. Build with `--features stealth` (needs `cmake` for the TLS stack).

## Test

```bash
curl -s -X POST http://localhost:8191/v1 \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"request.get","url":"https://nowsecure.nl","maxTimeout":60000}' \
  | jq '{status, message, cookies: [.solution.cookies[].name], html_len: (.solution.response|length)}'
```

Success for protected sites usually means a `cf_clearance` cookie in the response.

## API

Endpoint: `POST /` or `POST /v1`  
Health: `GET /health`

| Command | Status |
|---------|--------|
| `request.get` | Supported |
| `sessions.create` | Supported |
| `sessions.destroy` | Supported |
| `sessions.list` | Supported |
| `request.post` | Not implemented yet |

Example session flow:

```bash
# Create session
curl -s -X POST http://localhost:8191/v1 \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"sessions.create"}' | jq .

# Use session (reuse cookies)
curl -s -X POST http://localhost:8191/v1 \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"request.get","url":"https://example.com","session":"<id>","maxTimeout":60000}'

# Destroy session
curl -s -X POST http://localhost:8191/v1 \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"sessions.destroy","session":"<id>"}'
```

Response shape matches FlareSolverr:

```json
{
  "status": "ok",
  "message": "Challenge solved!",
  "solution": {
    "url": "https://example.com/",
    "status": 200,
    "response": "<html>...</html>",
    "userAgent": "Mozilla/5.0 ...",
    "cookies": [{ "name": "cf_clearance", "value": "..." }]
  }
}
```

## CLI

```bash
obscura solverr --help
```

| Flag | Default | Description |
|------|---------|-------------|
| `--host` | `0.0.0.0` | Bind address |
| `--port` | `8191` | HTTP port |
| `--stealth` | off | Enable anti-detect + TLS impersonation (use this) |
| `--proxy` | — | HTTP/SOCKS5 proxy for all sessions |
| `--user-agent` | — | Custom User-Agent |
| `--max-session-requests` | `25` | Requests per session before recreate |

Global flags also apply: `--proxy`, `--stealth`, `--user-agent`.

## Environment

Solver sets these defaults on startup if unset (longer deadlines for Cloudflare JS):

| Variable | Solver default | Purpose |
|----------|----------------|---------|
| `OBSCURA_SCRIPT_DEADLINE_MS` | `60000` | Max time for page script execution |
| `OBSCURA_NAV_TIMEOUT_MS` | `60000` | Navigation timeout |
| `OBSCURA_FETCH_TIMEOUT_MS` | `60000` | JS fetch/XHR timeout |

See [docs/Environment-variables.md](docs/Environment-variables.md) and [docs/Configure-stealth-and-proxies.md](docs/Configure-stealth-and-proxies.md) for the full Obscura engine reference.

## Integrations

**mangadl-go** — point FlareSolverr URL at ObscuraSolverr (no code changes):

```yaml
# docker-compose
command: [..., "-flaresolverr", "http://obscura-solverr:8191"]
```

**Prowlarr / *arr** — add indexer proxy type **FlareSolverr**, host `http://obscura-solverr:8191`.

## Other Obscura commands

This fork still includes the upstream Obscura CLI (`fetch`, `serve`, `scrape`, `mcp`, CDP on `:9222`). See [docs/](docs/) and [upstream README](https://github.com/h4ckf0r0day/obscura) for browser automation details.

## Project layout

| Path | Purpose |
|------|---------|
| `crates/obscura-solverr/` | FlareSolverr HTTP API + challenge wait logic |
| `crates/obscura-*` | Obscura browser engine (upstream) |
| `compose.solverr.yaml` | Docker Compose for `:8191` |

## License

Apache 2.0 — same as upstream Obscura.

Based on [h4ckf0r0day/obscura](https://github.com/h4ckf0r0day/obscura).
