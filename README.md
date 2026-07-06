# klyn

A Redis-compatible in-memory key-value store written in Rust from scratch.

klyn speaks [RESP](https://redis.io/docs/latest/develop/reference/protocol-spec/) (Redis Serialization Protocol), meaning any Redis client can connect to it out of the box.

## Why?

Most key-value store projects wrap an existing Redis library and call it a day. klyn doesn't. Every byte of protocol parsing, every TCP connection, every command dispatch — all written from scratch using only `std::net`, `std::io`, and `std::collections::HashMap`.

The goal: understand what actually happens when a client sends `SET foo bar` to a database. The TCP handshake. The byte-level protocol parsing. The in-memory storage. The response encoding. All of it, visible and readable.

## Features

- ✅ RESP protocol parser (handles Simple Strings, Errors, Integers, Bulk Strings, Arrays)
- ✅ RESP protocol encoder (serializes frames back to wire format)
- ✅ TCP server on port 6379
- ✅ Compatible with `redis-cli` and any RESP-speaking client
- ✅ In-memory storage with `HashMap<String, String>`
- ✅ Null bulk string responses (`$-1\r\n`) for missing keys
- ✅ Proper error responses for unknown commands

### Supported Commands

| Command | Syntax | Response |
|---------|--------|----------|
| `PING` | `PING` | `+PONG\r\n` |
| `SET` | `SET key value` | `+OK\r\n` |
| `GET` | `GET key` | `$<len>\r\n<value>\r\n` or `$-1\r\n` (nil) |
| `DEL` | `DEL key` | `:1\r\n` or `:0\r\n` |
| `EXISTS` | `EXISTS key` | `:1\r\n` or `:0\r\n` |
| `KEYS` | `KEYS` | `*<count>\r\n<keys...>` or `*0\r\n` |

## Quick Start

```bash
# Build and run
cargo run

# In another terminal, connect with redis-cli
redis-cli -p 6379

127.0.0.1:6379> PING
PONG
127.0.0.1:6379> SET name emmanuel
OK
127.0.0.1:6379> GET name
"emmanuel"
127.0.0.1:6379> EXISTS name
(integer) 1
127.0.0.1:6379> KEYS
1) "name"
127.0.0.1:6379> DEL name
(integer) 1
127.0.0.1:6379> GET name
(nil)
```

Or test with raw RESP bytes:

```bash
echo -ne '*1\r\n$4\r\nPING\r\n' | nc localhost 6379
# Output: +PONG\r\n
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                        klyn                             │
│                                                         │
│  ┌──────────────┐    ┌──────────────┐    ┌───────────┐  │
│  │  TCP Listener│───▶│  RESP Parser │───▶│  Command  │  │
│  │  (std::net)  │    │  (main.rs)   │    │  Router   │  │
│  └──────────────┘    └──────────────┘    └─────┬─────┘  │
│                                                 │       │
│                          ┌──────────────────────┴─────┐ │
│                          │       Storage Layer        │ │
│                          │  HashMap<String, String>   │ │
│                          └────────────────────────────┘ │
│                                 │                       │
│                          ┌──────▼───────┐               │
│                          │  RESP Encoder│               │
│                          │  (main.rs)   │               │
│                          └──────┬───────┘               │
│                                 │                       │
│                          ┌──────▼───────┐               │
│                          │  TCP Stream  │               │
│                          │  (write_all) │               │
│                          └──────────────┘               │
└─────────────────────────────────────────────────────────┘
```

**Data flow:** TCP bytes arrive → parsed into a `Frame` enum → command router matches on the first array element → storage layer executes the command → response `Frame` is encoded back to RESP bytes → written to the TCP stream.

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

The parser reads raw bytes and produces a `Frame`. The encoder takes a `Frame` and produces bytes. The command router pattern-matches on `Frame::Array` variants to dispatch commands.

### Protocol Parsing

RESP is a line-oriented protocol with length-prefixed bulk strings. The parser:

1. Reads the first byte to determine the frame type (`+`, `-`, `:`, `$`, `*`)
2. Parses the content according to that type's rules
3. Returns the parsed `Frame` along with any remaining bytes (for pipelined commands)

Array parsing is recursive — each element is itself a frame, parsed by the same function. This allows nested structures like `*2\r\n*1\r\n$4\r\nPING\r\n$3\r\nfoo\r\n`.

## What I Learned

- **TCP networking from scratch** — `TcpListener`, `TcpStream`, reading raw bytes from sockets, writing responses back. No HTTP framework, no abstraction layer.
- **Protocol design and parsing** — reading a spec, implementing a wire protocol byte-by-byte, handling length-prefixed data, building a recursive parser for nested structures.
- **Rust ownership and borrowing** — move semantics, `&str` vs `String`, `Option<T>`, pattern matching with guards, `#[derive(Debug, PartialEq)]`.
- **Enum-driven design** — using Rust's algebraic enums to model protocol types, matching on variants to dispatch behavior.
- **Standard library fluency** — `HashMap`, `Vec`, iterators, `split`, `collect`, `map`, closures — all without external dependencies.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (edition 2021) |
| Networking | `std::net::TcpListener` / `TcpStream` |
| I/O | `std::io::Read` / `Write` traits |
| Storage | `std::collections::HashMap` |
| Dependencies | None — zero external crates |

## Roadmap

- [ ] Concurrent client handling (`Arc<Mutex<HashMap>>` + `thread::spawn`)
- [ ] Disk persistence (append-only file / RDB snapshots)
- [ ] TTL and key expiration (`EXPIRE`, `TTL`, `PERSIST`)
- [ ] Additional commands (`MSET`, `MGET`, `INCR`, `DECR`, `FLUSHDB`)
- [ ] Pipelining (multiple commands in a single TCP read)
- [ ] Async I/O with tokio
