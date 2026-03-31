# Rust Search Crawler Lab


<p align="center">
  <img src="assets/the-sun-god.jpg" width="50%" />
</p>

> Primary documentation is available in English; a Portuguese translation follows below.

This repository is an open lab for experimenting with a Rust crawler and evolving a small personal search engine. The goal is to stay intentionally approachable (“a simple crawler lab in Rust for a search engine”) while still providing enough structure to tinker with crawling, parsing, indexing, dashboards, and deployment.

## Flow Overview
1. **Configuration** – `AppConfig` defines config values (seeds, delays, timeouts, user agent, database URL, API token).
2. **Database** – `Database` ensures the Postgres schema (via `sqlx` migrations), manages the prioritized queue and retry attempts, persists pages and events in atomic transactions, and exposes helpers for full-text search using `tsvector`.
3. **Fetch** – `Fetcher` obeys `robots.txt`, enforces global/per-host rate limits, validates content type/size, and reuses a single `reqwest::Client` instance.
4. **Parsing** – `Parser` strips scripts/styles, extracts title, description, headings, language, summary, and normalized links.
5. **Scheduler** – `Crawler` coordinates a parallel loop with global and per-host concurrency limits, updates statuses, runs fetch + parse + store, and keeps retry/backoff metrics.
6. **Serving** – `Axum` powers the HTTP API plus an SSE stream, applying auth before CORS so dashboards and browsers receive the correct headers even on 401s.
7. **Search** – SQL queries run against Postgres `tsvector` columns to deliver instant keyword lookups.
8. **Observability** – `logging` configures `tracing` subscribers for structured logs and metrics spans.

## Directory Layout
```
src/
├── config/       # AppConfig loader and CLI/env handling
├── db/           # Postgres pool, migrations, queue helpers, FTS queries
├── events/       # Event log models/helpers for SSE and persistence
├── fetcher/      # HTTP client, politeness logic, robots.txt checks
├── logging/      # tracing subscriber bootstrap
├── models/       # Shared structs (PageRecord, QueueItem, summaries)
├── parser/       # HTML parsing and metadata extraction
├── scheduler/    # Crawl loop, retry/backoff control, coordination
├── server/       # Axum routes (REST + SSE) and middleware stack
└── main.rs       # CLI entrypoint (`crawl` vs `serve`), config wiring

dashboard/        # Next.js dashboard with Tailwind
config/           # JSON config presets for local/dev/docker
```

## Modules
- `config`: defines `AppConfig`, loads layered JSON/env/CLI values, and shares them with both binaries.
- `db`: creates connection pools, runs migrations, manages the crawl queue, event log, and search queries with `sqlx::postgres`.
- `events`: describes event payloads, streaming helpers, and SSE serialization logic.
- `fetcher`: reuses a single HTTP client, honors politeness constraints, filters response metadata, and reports attempts/failures.
- `parser`: uses `scraper` and language detection to produce normalized metadata (title, description, headings, summary, locale) plus full text.
- `scheduler`: orchestrates the concurrent crawl loop, enforces priorities/domains, schedules retries with attempts stored in Postgres, and emits structured events.
- `server`: exposes REST endpoints (`/api/*`) and `/stream/events`, handles API token auth (header or query), and keeps the SSE feed alive with DB polling.
- `logging`: sets up `tracing-subscriber` with `EnvFilter`, compact timers, and `RUST_LOG` integration.
- `models`: centralizes shared structs/enums for queue items, pages, and API DTOs.

## Running

You need a running Postgres instance (local Docker works). Quick example:

```bash
docker run --rm --name crawler-postgres \
  -e POSTGRES_PASSWORD=crawler \
  -e POSTGRES_USER=crawler \
  -e POSTGRES_DB=crawler \
  -p 5432:5432 postgres:15
```

Update `appsettings.json` with `"database_url": "postgres://crawler:crawler@localhost:5432/crawler"` and run:

```bash
cargo run -- crawl --config appsettings.json
cargo run -- serve --config appsettings.json --addr 0.0.0.0:8080

# Direct CLI search against the index
cargo run -- crawl --config appsettings.json --search "async" --search-limit 5
RUST_LOG=debug cargo run -- crawl --config appsettings.json
```

### JSON Configuration
Inspired by .NET’s `appsettings.json`, configuration lives in a JSON file. The CLI looks for `appsettings.json` in the repo root by default, but you can override it via `--config path/to/custom.json` or the `CRAWLER_CONFIG=/absolute/path.json` environment variable.

Supported keys:

```json
{
  "database_url": "postgres://crawler:crawler@localhost:5432/crawler",
  "seeds": ["https://www.rust-lang.org/"],
  "user_agent": "RustyCrawlerMVP/0.1",
  "request_timeout_secs": 10,
  "politeness_delay_secs": 1,
  "default_priority": 0,
  "retry_max_attempts": 3,
  "retry_backoff_secs": 5,
  "host_delay_secs": 1,
  "max_response_bytes": 2000000,
  "allowed_content_types": ["text/html", "application/xhtml+xml"],
  "max_concurrency": 4,
  "max_host_parallelism": 1,
  "database_max_connections": 12,
  "api_token": null
}
```

- `database_url`: SQLx-compatible URL (Postgres only right now, e.g., `postgres://user:pass@host/db`).
- `seeds`: HTTP/HTTPS URLs that bootstrap the crawl (default `https://www.rust-lang.org/`).
- `user_agent`: identifier sent on every HTTP request.
- `request_timeout_secs`: per-request timeout (integer > 0).
- `politeness_delay_secs`: minimum delay between requests globally (integer > 0).
- `default_priority`: priority applied to seeds and new links (higher numbers are preferred).
- `retry_max_attempts`: maximum retry attempts before a URL is permanently marked as failed.
- `retry_backoff_secs`: base delay (seconds) applied between retries.
- `host_delay_secs`: delay enforced between requests to the same host (seconds > 0).
- `max_response_bytes`: maximum allowed response size; anything larger is discarded early.
- `allowed_content_types`: MIME types to accept (case-insensitive prefix match).
- `max_concurrency`: number of concurrent fetch/process jobs.
- `max_host_parallelism`: number of concurrent jobs allowed per host to stay polite.
- `database_max_connections`: pool size (use different values for crawler and serve modes).
- `api_token`: optional shared secret. When present, every HTTP call must include `X-API-Key` or an `api_token` query parameter with the same value.

Example with a custom file:

```bash
cargo run -- --config config/appsettings.production.json
```

### Querying the Index
After collecting data you can search Postgres directly (`tsvector` based):

```bash
cargo run -- --search "async await" --search-limit 5

# or pointing at a custom config file
cargo run -- --config config/appsettings.prod.json --search "timeline"
```

Results include the title, detected language, snippet with matches, and the canonical URL.

### HTTP Server / SSE
- `curl http://localhost:8080/api/metrics` — aggregate stats (total pages, queue statuses, failures).
- `curl "http://localhost:8080/api/queue?status=pending&limit=25"` — queue inspection with filters.
- `curl http://localhost:8080/api/events` — recent persisted events.
- `curl "http://localhost:8080/api/search?query=async&limit=5"` — full-text search proxy.
- `curl "http://localhost:8080/api/page?url=https://www.rust-lang.org/"` — full page details.
- `curl http://localhost:8080/stream/events` — SSE stream with `started/succeeded/failed/retrying` in real time.

If `api_token` is set, add `-H "X-API-Key: <token>"` or append `?api_token=<token>` to each request.

### Dashboard

The dashboard lives under `dashboard/`:

```bash
cd dashboard
npm install
NEXT_PUBLIC_CRAWLER_API_URL=http://localhost:8080 \
NEXT_PUBLIC_CRAWLER_API_TOKEN=changeme \
npm run dev
# In another terminal: cargo run -- serve --config appsettings.json (with api_token set)
```

Routes `/dashboard`, `/queue`, `/search`, and `/pages/[url]` talk to the API endpoints (including SSE) exposed by `cargo run -- serve` to render metrics, queue state, and live events.

### Docker Compose

`docker-compose.yml` provisions Postgres, crawler, API, and dashboard containers:

```bash
docker compose up --build
# API on http://localhost:8080 and dashboard on http://localhost:3000
```

Services read from `config/crawler.docker.json` and `config/server.docker.json` (pointing at `postgres://crawler:crawler@postgres:5432/crawler`). Adjust seeds, limits, and `api_token` before running Compose.

## Roadmap Snapshot
See `ROADMAP.md` for the evolution plan (advanced configuration, queue hardening, smarter fetcher with `robots.txt`, richer parsing, indexing, concurrency knobs, observability, and tests). `ROADMAP_DOCKERIZACAO.md` tracks Docker- and deployment-specific tasks (Postgres, dual binaries, dashboard, Compose setup).

---

Este repositório é um laboratório aberto para experimentar com um crawler em Rust e evoluir um pequeno motor de busca pessoal. A ideia é manter tudo intencionalmente acessível (“um simple crawler lab em Rust para um buscador”), mas com estrutura suficiente para testar coleta, parsing, indexação, dashboards e deploy.

### Visão Geral do Fluxo
1. **Configuração** – `AppConfig` define seeds, atrasos, timeouts, user-agent, URL do banco e token opcional.
2. **Banco** – `Database` garante o schema do Postgres (via migrações `sqlx`), administra a fila priorizada e tentativas, persiste páginas/eventos em transações e fornece helpers de busca full-text com `tsvector`.
3. **Fetch** – `Fetcher` respeita `robots.txt`, aplica limites globais/por host, valida Content-Type/tamanho e reutiliza um único `reqwest::Client`.
4. **Parsing** – `Parser` remove scripts/estilos e extrai título, descrição, headings, idioma, resumo e links normalizados.
5. **Scheduler** – `Crawler` coordena o loop paralelo com limites globais e por host, atualiza status, executa fetch + parse + storage e acompanha métricas de retry/backoff.
6. **Serving** – `Axum` expõe a API HTTP e o stream SSE, aplicando autenticação antes do CORS para que navegadores recebam headers mesmo em 401.
7. **Busca** – Consultas usam colunas `tsvector` do Postgres para retornar resultados rápidos.
8. **Observabilidade** – `logging` configura `tracing` para logs estruturados e spans de métricas.

### Estrutura de Pastas
```
src/
├── config/       # Loader do AppConfig e suporte para CLI/env
├── db/           # Pool Postgres, migrações, fila, helpers de busca
├── events/       # Modelos/helpers de eventos para SSE e persistência
├── fetcher/      # Cliente HTTP, regras de cortesia, robots.txt
├── logging/      # Bootstrap do tracing-subscriber
├── models/       # Structs compartilhadas (PageRecord, QueueItem, etc.)
├── parser/       # Parsing de HTML e extração de metadados
├── scheduler/    # Loop de crawl, retries/backoff, coordenação
├── server/       # Rotas Axum (REST + SSE) e middleware
└── main.rs       # Entrada CLI (`crawl` vs `serve`), wiring de config

dashboard/        # Dashboard Next.js com Tailwind
config/           # Presets JSON para local/dev/docker
```

### Módulos
- `config`: define `AppConfig`, carrega valores combinando JSON/env/CLI e compartilha entre os binários.
- `db`: cria pools, roda migrações, gerencia fila/event log/busca via `sqlx::postgres`.
- `events`: descreve payloads de evento, helpers de stream e serialização SSE.
- `fetcher`: reutiliza um único cliente HTTP, aplica limites de cortesia, filtra respostas e reporta tentativas/falhas.
- `parser`: usa `scraper` e detecção de idioma para gerar metadados normalizados e texto limpo.
- `scheduler`: orquestra o loop concorrente, respeita prioridades/domínios, agenda retries com contagem no Postgres e emite eventos estruturados.
- `server`: expõe endpoints REST (`/api/*`) e `/stream/events`, trata o token de API (header ou query) e mantém o SSE atualizado com polling no banco.
- `logging`: configura `tracing-subscriber` com `EnvFilter`, timers compactos e suporte a `RUST_LOG`.
- `models`: centraliza structs/enums compartilhados (fila, páginas, DTOs da API).

### Execução

Você precisa de um Postgres ativo (Docker local resolve):

```bash
docker run --rm --name crawler-postgres \
  -e POSTGRES_PASSWORD=crawler \
  -e POSTGRES_USER=crawler \
  -e POSTGRES_DB=crawler \
  -p 5432:5432 postgres:15
```

Atualize `appsettings.json` com `"database_url": "postgres://crawler:crawler@localhost:5432/crawler"` e rode:

```bash
cargo run -- crawl --config appsettings.json
cargo run -- serve --config appsettings.json --addr 0.0.0.0:8080

# Busca direta via CLI
cargo run -- crawl --config appsettings.json --search "async" --search-limit 5
RUST_LOG=debug cargo run -- crawl --config appsettings.json
```

#### Configuração em JSON
Inspirado no `appsettings.json` do .NET, toda configuração fica em JSON. O CLI procura `appsettings.json` na raiz por padrão, mas você pode usar `--config caminho/custom.json` ou `CRAWLER_CONFIG=/abs/path.json`.

Campos suportados:

```json
{
  "database_url": "postgres://crawler:crawler@localhost:5432/crawler",
  "seeds": ["https://www.rust-lang.org/"],
  "user_agent": "RustyCrawlerMVP/0.1",
  "request_timeout_secs": 10,
  "politeness_delay_secs": 1,
  "default_priority": 0,
  "retry_max_attempts": 3,
  "retry_backoff_secs": 5,
  "host_delay_secs": 1,
  "max_response_bytes": 2000000,
  "allowed_content_types": ["text/html", "application/xhtml+xml"],
  "max_concurrency": 4,
  "max_host_parallelism": 1,
  "database_max_connections": 12,
  "api_token": null
}
```

- `database_url`: URL suportada pelo SQLx (Postgres, ex.: `postgres://user:pass@host/db`).
- `seeds`: URLs HTTP/HTTPS que iniciam o crawl (default `https://www.rust-lang.org/`).
- `user_agent`: identificador enviado em cada requisição.
- `request_timeout_secs`: timeout por requisição (inteiro > 0).
- `politeness_delay_secs`: atraso mínimo global entre requisições.
- `default_priority`: prioridade inicial aplicada a seeds/links (maior = preferência).
- `retry_max_attempts`: máximo de tentativas antes de falhar definitivamente.
- `retry_backoff_secs`: atraso base entre retries (segundos).
- `host_delay_secs`: atraso mínimo por host para manter cortesia.
- `max_response_bytes`: tamanho máximo aceito na resposta.
- `allowed_content_types`: lista de MIME types aceitos (prefix match, case-insensitive).
- `max_concurrency`: número máximo de jobs concorrentes.
- `max_host_parallelism`: jobs simultâneos permitidos por host.
- `database_max_connections`: tamanho do pool (use valores distintos para crawler e API).
- `api_token`: opcional; quando definido, todas as chamadas HTTP precisam de `X-API-Key` ou `?api_token=` com o mesmo valor.

Exemplo com arquivo customizado:

```bash
cargo run -- --config config/appsettings.production.json
```

#### Consultando o Índice
Depois de coletar dados, é possível buscar direto no Postgres (`tsvector`):

```bash
cargo run -- --search "async await" --search-limit 5

# ou usando outro arquivo
cargo run -- --config config/appsettings.prod.json --search "linha do tempo"
```

Os resultados exibem título, idioma detectado, snippet com o match e a URL canônica.

#### Servidor HTTP / SSE
- `curl http://localhost:8080/api/metrics` — agregados (total de páginas, fila pendente/em andamento/falhas).
- `curl "http://localhost:8080/api/queue?status=pending&limit=25"` — inspeção da fila com filtros.
- `curl http://localhost:8080/api/events` — feed recente de eventos persistidos.
- `curl "http://localhost:8080/api/search?query=async&limit=5"` — proxy para a busca full-text.
- `curl "http://localhost:8080/api/page?url=https://www.rust-lang.org/"` — detalhes completos de uma página.
- `curl http://localhost:8080/stream/events` — stream SSE com `started/succeeded/failed/retrying` em tempo real.

Se `api_token` estiver definido, inclua `-H "X-API-Key: <token>"` ou `?api_token=<token>` em cada chamada.

#### Dashboard

O painel mora em `dashboard/`:

```bash
cd dashboard
npm install
NEXT_PUBLIC_CRAWLER_API_URL=http://localhost:8080 \
NEXT_PUBLIC_CRAWLER_API_TOKEN=changeme \
npm run dev
# Em outro terminal: cargo run -- serve --config appsettings.json (com api_token definido)
```

As rotas `/dashboard`, `/queue`, `/search` e `/pages/[url]` consomem a API (incluindo SSE) exposta pelo `cargo run -- serve` para renderizar métricas, fila e eventos ao vivo.

#### Docker Compose

`docker-compose.yml` provisiona Postgres, crawler, API e dashboard:

```bash
docker compose up --build
# API em http://localhost:8080 e dashboard em http://localhost:3000
```

Os serviços usam `config/crawler.docker.json` e `config/server.docker.json` (com `postgres://crawler:crawler@postgres:5432/crawler`). Ajuste seeds, limites e `api_token` antes de subir o Compose.

### Roadmap Resumido
Veja `ROADMAP.md` para o plano macro (config avançada, fila robusta, fetcher com robots.txt, parsing aprimorado, indexação, paralelismo, observabilidade e testes). O arquivo `ROADMAP_DOCKERIZACAO.md` lista as tarefas específicas de Docker/deploy (Postgres, binários duplos, dashboard, Compose).
