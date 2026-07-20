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
- ✅ 17 unit tests covering the storage layer

### Supported Commands

| Command | Syntax | Response |
|---------|--------|----------|
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

```bash
# Build and run
cargo run
```

```bash
# In another terminal — one-shot redis-cli commands (each opens its own connection)
redis-cli -p 6379 PING                    # PONG
redis-cli -p 6379 SET name emmanuel       # OK
redis-cli -p 6379 GET name                # "emmanuel"
redis-cli -p 6379 INCR counter            # (integer) 1
redis-cli -p 6379 INCR counter            # (integer) 2
redis-cli -p 6379 EXPIRE counter 60       # (integer) 1
redis-cli -p 6379 TTL counter             # (integer) 59
redis-cli -p 6379 PERSIST counter         # (integer) 1
redis-cli -p 6379 KEYS                    # 1) "name"  2) "counter"
redis-cli -p 6379 DEL name                # (integer) 1
```

Or test with raw RESP bytes:

```bash
printf '*1\r\n$4\r\nPING\r\n' | nc localhost 6379
# +PONG
```

Restart the server and your `SET`/`DEL`/`FLUSHDB` history is replayed from `klyn.aof`.

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
├── tests.rs    # 17 unit tests for the storage layer
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

Each incoming connection gets its own `std::thread`. The database is an `Arc<Mutex<HashMap<String, (String, Option<Instant>)>>>` — the tuple stores the value plus an optional expiry timestamp. The AOF file handle is shared the same way, so every write command is serialized through the mutex before hitting disk.

### Expiration Model

Expiration is **lazy** (passive): keys are checked against their `Instant` deadline only when accessed via `GET`, `TTL`, `PERSIST`, or `KEYS`. An expired key that is touched is deleted on the spot and the deletion is appended to the AOF. There is no background sweeper yet — expired keys that are never accessed linger in memory.

### Persistence Model

`SET`, `DEL`, and `FLUSHDB` are re-encoded as RESP arrays and appended to `klyn.aof`. On startup, the file is split into frames and replayed: `SET` inserts, `DEL` removes, `FLUSHDB` clears. Because the log is just RESP, you can inspect it with `cat klyn.aof` and read your own database history.

## Testing

```bash
cargo test
```

17 unit tests cover the storage layer: set/get, delete semantics, existence checks, counter behavior (including the non-integer error path), expiry lifecycle (`EXPIRE` → `TTL` → `PERSIST`), key listing, and database flushing. Tests use an isolated AOF at `/tmp/klyn_test.aof` so they never touch real data.

## Known Limitations

Honest trade-offs, in roughly the order I plan to fix them:

- **One command per connection.** The handler performs a single `read()` and returns, so the connection closes after one command and interactive `redis-cli` sessions drop. Pipelined frames sent in one write are parsed but discarded.
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

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (edition 2024) |
| Networking | `std::net::TcpListener` / `TcpStream` |
| Concurrency | `std::thread` + `Arc` / `Mutex` |
| I/O | `std::io::Read` / `Write` traits |
| Storage | `std::collections::HashMap` |
| Persistence | Append-only file (`klyn.aof`), RESP-encoded |
| Dependencies | None — zero external crates |
