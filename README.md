# klyn

A Redis-compatible in-memory key-value store written in Rust from scratch.

klyn speaks [RESP](https://redis.io/docs/latest/develop/reference/protocol-spec/) (Redis Serialization Protocol), meaning any Redis client can connect to it out of the box. It supports multiple concurrent clients, persists writes to an append-only file, and restores state on restart.

## Why?

Most key-value store projects wrap an existing Redis library and call it a day. klyn doesn't. Every byte of protocol parsing, every TCP connection, every command dispatch, every persisted write — all written from scratch using only `std::net`, `std::io`, and `std::collections::HashMap`. **Zero external dependencies.**

The goal: understand what actually happens when a client sends `SET foo bar` to a database. The TCP handshake. The byte-level protocol parsing. The in-memory storage. The response encoding. The crash recovery. All of it, visible and readable.

## Features

- ✅ RESP protocol parser (Simple Strings, Errors, Integers, Bulk Strings, Arrays — recursive for nested arrays)
- ✅ RESP protocol encoder (serializes frames back to wire format)
- ✅ TCP server on port 6379, compatible with `redis-cli` and any RESP-speaking client
- ✅ Concurrent clients — thread-per-connection with `Arc<Mutex<HashMap>>`
- ✅ AOF (append-only file) persistence — write commands logged in RESP format, replayed on startup
- ✅ TTL / key expiration — lazy expiry with `EXPIRE`, `TTL`, `PERSIST`
- ✅ Atomic counters — `INCR` / `DECR` with proper error handling for non-integer values
- ✅ 21 unit tests covering the storage layer

### Supported Commands

| Command | Syntax | Response |
| --------- | -------- | ---------- |
| `PING` | `PING` | `+PONG\r\n` |
| `SET` | `SET key value` | `+OK\r\n` |
| `GET` | `GET key` | `$<len>\r\n<value>\r\n` or `$-1\r\n` (nil) |
| `DEL` | `DEL key` | `:1\r\n` or `:0\r\n` |
| `EXISTS` | `EXISTS key` | `:1\r\n` or `:0\r\n` |
| `KEYS` | `KEYS` | `*<count>\r\n<keys...>` or `*0\r\n` |
| `INCR` | `INCR key` | `:<new value>\r\n` or error if not an integer |
| `DECR` | `DECR key` | `:<new value>\r\n` or error if not an integer |
| `EXPIRE` | `EXPIRE key seconds` | `:1\r\n` or `:0\r\n` |
| `TTL` | `TTL key` | seconds remaining, `-1` (no expiry), or `-2` (no key) |
| `PERSIST` | `PERSIST key` | `:1\r\n` (expiry removed) or `:0\r\n` |
| `FLUSHDB` | `FLUSHDB` | `+OK\r\n` |

## Quick Start

### Prerequisites

Building needs only a Rust toolchain — the server itself has zero dependencies.

To **use** it you need a RESP client. klyn speaks the Redis wire protocol, so Redis's own tooling drives it:

| Platform | Install |
| --- | --- |
| Arch | `sudo pacman -S valkey` |
| Debian / Ubuntu | `sudo apt install redis-tools` |
| macOS | `brew install redis` |

On Arch the package is called `valkey` — it provides the `redis` name and ships `redis-server`, `redis-cli` and `redis-benchmark`. Elsewhere the same tools come from the Redis packages.

That gives you `redis-cli` (interactive client) and `redis-benchmark` (load generator).

If you'd rather install nothing, raw RESP over `nc` works for simple checks — see below.

### Build and run

```bash
cargo build --release
./target/release/klyn
```

### Talk to it

Interactive session — `redis-cli` keeps one connection open and sends one command per line:

```bash
redis-cli -p 6379
```

```
127.0.0.1:6379> PING
PONG
127.0.0.1:6379> SET name emmanuel
OK
127.0.0.1:6379> GET name
"emmanuel"
127.0.0.1:6379> INCR counter
(integer) 1
127.0.0.1:6379> INCR counter
(integer) 2
127.0.0.1:6379> EXPIRE counter 60
(integer) 1
127.0.0.1:6379> TTL counter
(integer) 59
127.0.0.1:6379> PERSIST counter
(integer) 1
127.0.0.1:6379> KEYS
1) "name"
2) "counter"
127.0.0.1:6379> DEL name
(integer) 1
```

Or one-shot from your shell:

```bash
redis-cli -p 6379 SET name emmanuel
redis-cli -p 6379 GET name
```

Or test with raw RESP bytes, no client at all:

```bash
printf '*1\r\n$4\r\nPING\r\n' | nc localhost 6379
# +PONG
```

Restart the server and your `SET`/`DEL`/`FLUSHDB` history is replayed from `klyn.aof`.

## Performance

Benchmarked against **Valkey 9.1.2** — the open-source fork of Redis 7.2 that Arch Linux ships in place of Redis — on the same machine, both servers running with an append-only log enabled from an empty keyspace. Full setup, every run and the discarded attempts: [BENCH.md](BENCH.md).

`redis-benchmark -n 200000 -c 8 -t set -r 100000` — 200,000 `SET`s over 8 persistent connections across 100,000 distinct keys.

| Metric | klyn | Valkey 9.1.2 |
| --- | --- | --- |
| Throughput | 58,928 req/s | ~70,300 req/s |
| p50 latency | 0.087 ms | 0.087 ms |
| p99 latency | 0.279 ms | 0.227 ms |
| Worst observed | 9.551 ms | 1.15–1.30 ms |
| CPU consumed | 1.94 cores | ~0.85 cores |
| CPU per request | 32.9 µs | ~12.5 µs |

**Median latency is identical.** Both answer the typical request in 87 µs. At this scale a request costs a loopback round trip rather than a data-structure operation — which is why keyspace size barely matters: one distinct key and 100,000 distinct keys differ by 2.3% in throughput.

**Throughput is bought with CPU.** klyn reaches 84% of Valkey's throughput for roughly 2.5× the CPU per request. Valkey executes commands on a single thread; klyn spawns a thread per connection, but the global mutex described in the [Concurrency Model](#concurrency-model) serialises every command that touches state. The threads therefore parallelise parsing, encoding and socket I/O rather than the data structure — measured at 1.94 cores under load, not 8.

**The tail is where it loses.** The slowest request is 7× Valkey's. About one request in 6,000 exceeds 1 ms against an 87 µs median. The AOF is written on the command path while the lock is held, so a slow write stalls the request that triggered it. *The spike is measured; its cause is not isolated.*

**Scaling.** Throughput peaks at 8 concurrent clients, matching the 8 hardware threads, then flattens (−4% at 50 clients) while p50 latency grows 5×. That shape is a saturated queue rather than a slow server. The 1-to-8 client speedup of 3.2× corresponds to roughly a 22% serial fraction, which caps the achievable gain at about 4.6× regardless of core count.

**Caveats.** Client and server share the same 4-core machine, so both figures include contention with the load generator. All traffic is loopback, which hides network-path effects such as Nagle and delayed ACK. Valkey 9.1.2 descends from Redis 7.2.4, the last BSD-licensed release, so it is a newer and faster baseline than Redis 7 itself. Throughput reproduced within 2% across three independent runs.

## Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                             klyn                               │
│                                                                │
│   ┌───────────────┐   ┌──────────────┐   ┌─────────────────┐   │
│   │  TCP Listener │──▶│  RESP Parser │──▶│  Command Router │   │
│   │  (main.rs)    │   │  (parser.rs) │   │  (main.rs)      │   │
│   │ thread/conn   │   └──────────────┘   └────────┬────────┘   │
│   └───────┬───────┘                               │            │
│           │                  ┌────────────────────▼───────┐    │
│           │                  │      Storage Layer         │    │
│           │                  │        (db.rs)             │    │
│           │                  │  HashMap<String, (String,  │    │
│           │                  │      Option<Instant>)>     │    │
│           │                  └───┬────────────────┬───────┘    │
│           │                      │                │            │
│           │               ┌──────▼───────┐  ┌─────▼────────┐   │
│           │               │ RESP Encoder │  │     AOF      │   │
│           │               │ (encoder.rs) │  │  (klyn.aof)  │   │
│           │               └──────┬───────┘  └──────────────┘   │
│           │                      │                             │
│           │               ┌──────▼───────┐                     │
│           └──────────────▶│  TCP Stream  │                     │
│                           │  (write_all) │                     │
│                           └──────────────┘                     │
└────────────────────────────────────────────────────────────────┘
```

**Data flow:** TCP bytes arrive → parsed into a `Frame` enum → command router matches on the first array element → storage layer executes against the shared `HashMap` (and appends mutations to the AOF) → response `Frame` is encoded back to RESP bytes → written to the TCP stream.

**Startup flow:** open `klyn.aof` → replay every logged frame through the parser → rebuild the `HashMap` → start listening.

### Project Structure

```
src/
├── frame.rs    # The Frame enum — one variant per RESP type
├── parser.rs   # Raw text → Frame (recursive for arrays)
├── encoder.rs  # Frame → RESP wire bytes
├── db.rs       # Command implementations against the shared HashMap
├── tests.rs    # 21 unit tests for the storage layer
└── main.rs     # TCP listener, thread spawn, AOF replay, command router
```

### The Frame Enum

The core data structure. Every RESP type maps to a Rust enum variant:

```rust
enum Frame {
    SimpleString(String),       // +OK\r\n
    SimpleError(String),        // -ERR message\r\n
    BulkString(Option<String>), // $5\r\nhello\r\n  or  $-1\r\n (nil)
    Array(Vec<Frame>),          // *2\r\n...elements...
    Integer(i32),               // :1\r\n
}
```

The parser reads raw input and produces a `Frame`. The encoder takes a `Frame` and produces bytes. The command router pattern-matches on `Frame::Array` variants to dispatch commands. Array parsing is recursive — each element is itself a frame — and the parser returns unconsumed input alongside the frame, which lays the groundwork for pipelining.

### Concurrency Model

Each incoming connection gets its own `std::thread`. The database is an `Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>` — the tuple stores the value plus an optional expiry timestamp. The AOF file handle is shared the same way, so every write command is serialized through the mutex before hitting disk. That serialisation is what the benchmark measures: the server peaks at 1.94 cores on an 8-thread machine rather than scaling across all of them — see [Performance](#performance).

### Expiration Model

Expiration is **lazy** (passive): keys are checked against their `Instant` deadline only when accessed via `GET`, `TTL`, `PERSIST`, or `KEYS`. An expired key that is touched is deleted on the spot and the deletion is appended to the AOF. There is no background sweeper yet — expired keys that are never accessed linger in memory.

### Persistence Model

`SET`, `DEL`, and `FLUSHDB` are re-encoded as RESP arrays and appended to `klyn.aof`. On startup, the file is split into frames and replayed: `SET` inserts, `DEL` removes, `FLUSHDB` clears. Because the log is just RESP, you can inspect it with `cat klyn.aof` and read your own database history.

## Testing

```bash
cargo test
```

21 unit tests cover the storage layer: set/get, delete semantics, existence checks, counter behavior (including the non-integer error path), expiry lifecycle (`EXPIRE` → `TTL` → `PERSIST`), key listing, and database flushing. Tests use an isolated AOF at `/tmp/klyn_test.aof` so they never touch real data.

## Known Limitations

Honest trade-offs, in roughly the order I plan to fix them:

- **Pipelined commands are dropped.** Sequential commands on one connection work, but when several frames arrive in a single `write()` only the first is executed — the parser returns the unconsumed remainder and the handler discards it.
- **Not binary-safe.** The parser splits on `\r\n` as strings, so values containing `\r\n` or non-UTF-8 bytes will corrupt parsing.
- **Partial AOF coverage.** `INCR`, `DECR`, `EXPIRE`, and `PERSIST` mutate state but aren't logged yet, so those changes don't survive a restart.
- **Fixed 512-byte buffer.** Larger commands are truncated.
- **32-bit integers.** Redis uses 64-bit; klyn currently parses counters as `i32`.
- **Passive expiry only.** No active background expiration of stale keys.

## Roadmap

- [x] RESP parser and encoder from scratch
- [x] TCP server compatible with `redis-cli`
- [x] Core commands (`PING`, `SET`, `GET`, `DEL`, `EXISTS`, `KEYS`)
- [x] Concurrent client handling (`Arc<Mutex<HashMap>>` + `thread::spawn`)
- [x] AOF persistence with startup replay
- [x] TTL and key expiration (`EXPIRE`, `TTL`, `PERSIST`)
- [x] Counters (`INCR`, `DECR`) and `FLUSHDB`
- [x] Unit test suite
- [ ] Connection loop — multiple commands + pipelining per connection
- [ ] Byte-slice parser (`&[u8]`) for binary safety
- [ ] Batch commands (`MSET`, `MGET`) and more (`APPEND`, `GETSET`, `TYPE`)
- [ ] Active expiration (background sweeper thread)
- [ ] AOF rewrite / compaction
- [ ] RDB-style snapshots
- [ ] Async I/O with tokio

## What I Learned

- **TCP networking from scratch** — `TcpListener`, `TcpStream`, reading raw bytes from sockets, writing responses back. No HTTP framework, no abstraction layer.
- **Protocol design and parsing** — reading a spec, implementing a wire protocol byte-by-byte, handling length-prefixed data, building a recursive parser for nested structures.
- **Rust ownership and borrowing** — move semantics, `&str` vs `String`, `Option<T>`, pattern matching with guards, `#[derive(Debug, PartialEq)]`.
- **Shared-state concurrency** — `Arc` for shared ownership across threads, `Mutex` for interior mutability, locking discipline to avoid holding a guard across unrelated work.
- **Enum-driven design** — using Rust's algebraic enums to model protocol types, matching on variants to dispatch behavior.
- **Persistence fundamentals** — append-only logs, replay-based recovery, why databases log mutations instead of rewriting state.
- **Standard library fluency** — `HashMap`, `Vec`, `Instant`/`Duration`, `OpenOptions`, iterators — all without external dependencies.
- **Measurement** — benchmarking against a reference implementation, reading percentile latency rather than averages, and separating "my server is slow" from "my benchmark is measuring the client".

## Tech Stack

| Layer | Technology |
| ------- | ----------- |
| Language | Rust (edition 2024) |
| Networking | `std::net::TcpListener` / `TcpStream` |
| Concurrency | `std::thread` + `Arc` / `Mutex` |
| I/O | `std::io::Read` / `Write` traits |
| Storage | `std::collections::HashMap` |
| Persistence | Append-only file (`klyn.aof`), RESP-encoded |
| Build dependencies | None — zero external crates |
| Client tooling | Any RESP client (`redis-cli`, `redis-benchmark`) |
