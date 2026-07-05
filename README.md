# ObscuraSolverr

FlareSolverr-compatible Cloudflare bypass API, powered by the [Obscura](https://github.com/h4ckf0r0day/obscura) headless browser.

Drop-in replacement on port **8191** — same `POST /v1` contract as FlareSolverr. Works with [mangadl-go](https://github.com/Rhevin/mangadl-go), Prowlarr, Sonarr, Radarr, and any client that already speaks FlareSolverr.

## Why this fork

| | FlareSolverr | ObscuraSolverr |
|---|--------------|----------------|
| Browser | Chrome/Firefox + Puppeteer | Obscura V8 + `--stealth` |
| Idle RAM | ~800 MiB – 1.5 GiB | **~15 MiB** |
| Solve CPU | ~100% for 30–60s | ~100% for ~50s (one core) |
| API | `POST /v1` on `:8191` | Same |
| License | MIT | Apache 2.0 (Obscura upstream) |

This repo is an Obscura fork with a **`solverr`** subcommand: navigate, wait for Cloudflare challenges, return HTML + cookies.

## Quick start

### Docker — pull pre-built image (recommended)

Images are published to GHCR on git tags `v*` (e.g. `v1.0.0` → `latest`).

```bash
docker compose -f compose.solverr.yaml pull
docker compose -f compose.solverr.yaml up -d
```

Image: `ghcr.io/rhevin/obscura-solverr:latest`

Pin a release in `compose.solverr.yaml`:

```yaml
image: ghcr.io/rhevin/obscura-solverr:1.0.0
```

Make the GHCR package **public** after the first release: GitHub → **Packages** → `obscura-solverr` → **Change visibility**.

### Docker — build locally

First build compiles V8 + stealth TLS (~5–10 min). Needs Docker BuildKit.

```bash
docker compose -f compose.solverr.yaml up -d --build
```

### Binary

```bash
cargo build --release --features stealth --bin obscura

./target/release/obscura solverr --stealth --host 0.0.0.0 --port 8191
```

Stealth is required for TLS fingerprinting and anti-detect. Build with `--features stealth` (`cmake`, `build-essential` on Linux).

## Test

```bash
curl -s http://localhost:8191/health | jq .

curl -s -X POST http://localhost:8191/v1 \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"request.get","url":"https://nowsecure.nl","maxTimeout":120000}' \
  | jq '{status, message, cf: ([.solution.cookies[]? | select(.name=="cf_clearance") | .value] | first)}'
```

Success on protected sites means `"status": "ok"` and a non-empty `cf_clearance` cookie. Solves typically take **~50s** (fail-fast first pass + Turnstile retry).

If `jq` prints nothing, check raw JSON — errors have no `solution`:

```bash
curl -s -X POST http://localhost:8191/v1 \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"request.get","url":"https://nowsecure.nl","maxTimeout":120000}' \
  | jq '{status, message}'
```

## Resource usage

```bash
docker stats obscura-solverr
```

| State | CPU | RAM |
|-------|-----|-----|
| Idle | ~0% | ~15 MiB |
| CF solve | ~100% (one core) | spikes briefly, well under 768 MiB limit |

Compose defaults cap the container at **768 MiB RAM** and **1 CPU**. Adjust in `compose.solverr.yaml` if needed.

## API

Endpoint: `POST /` or `POST /v1`  
Health: `GET /health`

| Command | Status |
|---------|--------|
| `request.get` | Supported (`maxTimeout` 1–300000 ms, default 120000) |
| `sessions.create` | Supported |
| `sessions.destroy` | Supported |
| `sessions.list` | Supported |
| `request.post` | Not implemented yet |

Example session flow:

```bash
curl -s -X POST http://localhost:8191/v1 \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"sessions.create"}' | jq .

curl -s -X POST http://localhost:8191/v1 \
  -H 'Content-Type: application/json' \
  -d '{"cmd":"request.get","url":"https://example.com","session":"<id>","maxTimeout":120000}'

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
| `--stealth` | off | Enable anti-detect + TLS impersonation (**required**) |
| `--proxy` | — | HTTP/SOCKS5 proxy for all sessions |
| `--user-agent` | — | Custom User-Agent |
| `--max-session-requests` | `25` | Requests per session before recreate |

## Environment

Solver defaults (also set in `compose.solverr.yaml`). Per-request `maxTimeout` overrides script/nav/fetch budgets for that solve.

| Variable | Default | Purpose |
|----------|---------|---------|
| `OBSCURA_SOLVERR_INITIAL_SCRIPT_MS` | `45000` | Short first-pass script budget (fail-fast before CF retry) |
| `OBSCURA_SCRIPT_DEADLINE_MS` | `120000` | Full script budget on retry / when unset at startup |
| `OBSCURA_NAV_TIMEOUT_MS` | `130000` | End-to-end navigation ceiling |
| `OBSCURA_FETCH_TIMEOUT_MS` | `120000` | JS fetch/XHR timeout |

See [docs/Environment-variables.md](docs/Environment-variables.md) and [docs/Configure-stealth-and-proxies.md](docs/Configure-stealth-and-proxies.md) for the full Obscura engine reference.

## Integrations

### mangadl-go

Point FlareSolverr URL at ObscuraSolverr — no code changes:

```env
MANGADL_FLARESOLVERR_URL=http://obscura-solverr:8191
MANGADL_USE_FLARESOLVERR=true
```

Or in Settings → FlareSolverr. Use the Docker service name when both run on the same compose network.

### Prowlarr / *arr

Add indexer proxy type **FlareSolverr**, URL `http://obscura-solverr:8191`.

## Releases

Publish a Docker image to GHCR:

```bash
git tag v1.0.0
git push origin v1.0.0
```

GitHub Actions workflow **Publish obscura-solverr** builds `linux/amd64` + `linux/arm64` and pushes `ghcr.io/rhevin/obscura-solverr:latest` and `:1.0.0`.

## Other Obscura commands

This fork still includes the upstream Obscura CLI (`fetch`, `serve`, `scrape`, `mcp`, CDP on `:9222`). See [docs/](docs/) and [upstream Obscura](https://github.com/h4ckf0r0day/obscura) for browser automation details.

## Project layout

| Path | Purpose |
|------|---------|
| `crates/obscura-solverr/` | FlareSolverr HTTP API + challenge wait logic |
| `crates/obscura-*` | Obscura browser engine (upstream) |
| `compose.solverr.yaml` | Docker Compose for `:8191` |
| `.github/workflows/publish-obscura-solverr.yml` | GHCR image on `v*` tags |

## License

Apache 2.0 — same as upstream Obscura.

Based on [h4ckf0r0day/obscura](https://github.com/h4ckf0r0day/obscura).
