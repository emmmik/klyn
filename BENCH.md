# Benchmark log

Raw measurements behind the numbers in the README. Interpretation lives there;
this file records what was run and what came back.

## Environment

| | |
| --- | --- |
| CPU | AMD Ryzen 7 PRO 3700U, 4 cores / 8 threads |
| RAM | 16 GB |
| Power | AC |
| Governor | `schedutil` |
| klyn | release build, AOF enabled, `127.0.0.1:6379` |
| Redis | `Valkey server v=9.1.2 sha=7f1dffed:1 malloc=jemalloc-5.3.1 bits=64 build=ab3e536b46916906`,`--appendonly yes --appendfsync everysec --save ''`,`127.0.0.1:6380` |
| Load generator | `redis-benchmark`, same machine as the server |

Arch Linux ships Valkey in place of Redis, so redis-server, redis-cli and redis-benchmark on this machine are Valkey's. Valkey 9.1.2 descends from Redis 7.2.4, the last BSD-licensed release.

Each run started from one klyn process (`pgrep -a klyn`), an empty keyspace
(`FLUSHDB` / `FLUSHALL`) and an empty AOF.

## Commands

```bash
# concurrency sweep
redis-benchmark -h 127.0.0.1 -p <port> -n 100000 -c <clients> -t set,get

# distinct keys
redis-benchmark -h 127.0.0.1 -p <port> -n 200000 -c 8 -t set -r 100000

# CPU sample during a run
redis-benchmark -h 127.0.0.1 -p <port> -n 200000 -c 8 -t set -r 100000 &
sleep 1
top -b -n2 -d1 -p <pid> | tail -2
wait
```

## Run log

### 1 — klyn, concurrency sweep

100,000 ops, single key, AOF on. Latency in ms.

| clients | test | req/s | avg | min | p50 | p95 | p99 | max |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | SET | 19,512 | 0.045 | 0.024 | 0.047 | 0.079 | 0.095 | 3.407 |
| 1 | GET | 25,272 | 0.035 | 0.016 | 0.039 | 0.063 | 0.071 | 0.639 |
| 8 | SET | 62,383 | 0.090 | 0.024 | 0.087 | 0.143 | 0.215 | 1.911 |
| 8 | GET | 75,019 | 0.067 | 0.024 | 0.071 | 0.095 | 0.143 | 0.463 |
| 32 | SET | 60,496 | 0.303 | 0.032 | 0.287 | 0.455 | 0.735 | 3.671 |
| 32 | GET | 75,188 | 0.228 | 0.064 | 0.223 | 0.287 | 0.391 | 0.719 |
| 50 | SET | 59,737 | 0.463 | 0.032 | 0.439 | 0.639 | 1.095 | 6.711 |
| 50 | GET | 72,622 | 0.360 | 0.096 | 0.351 | 0.463 | 0.647 | 2.311 |

### 2 — Redis, concurrency sweep

100,000 ops, single key, AOF everysec.

| clients | test | req/s | avg | min | p50 | p95 | p99 | max |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | SET | 16,801 | 0.053 | 0.024 | 0.047 | 0.095 | 0.119 | 0.831 |
| 1 | GET | 19,956 | 0.044 | 0.024 | 0.047 | 0.079 | 0.095 | 4.591 |
| 8 | SET | 67,935 | 0.093 | 0.032 | 0.087 | 0.167 | 0.247 | 4.935 |
| 8 | GET | 71,582 | 0.072 | 0.024 | 0.063 | 0.127 | 0.183 | 0.519 |
| 32 | SET | 73,314 | 0.280 | 0.056 | 0.239 | 0.495 | 0.735 | 3.407 |
| 32 | GET | 73,421 | 0.254 | 0.088 | 0.215 | 0.431 | 0.631 | 1.447 |
| 50 | SET | 73,046 | 0.421 | 0.080 | 0.351 | 0.775 | 1.199 | 5.447 |
| 50 | GET | 72,202 | 0.384 | 0.080 | 0.335 | 0.639 | 0.863 | 2.167 |

### 3 — klyn vs Redis, 100,000 distinct keys

| run | test | req/s | p50 |
| --- | --- | ---: | ---: |
| klyn, `-r 100000`, 100k ops | SET | 59,032 | 0.087 |
| klyn, `-r 100000`, 100k ops | GET | 73,855 | 0.071 |
| klyn, `-r 100000`, 100k ops (repeat) | SET | 57,937 | 0.095 |
| klyn, `-r 100000`, 100k ops (repeat) | GET | 72,516 | 0.071 |
| Redis, `-r 100000`, 100k ops | SET | 70,522 | 0.087 |
| Redis, `-r 100000`, 100k ops | GET | 70,373 | 0.071 |

### 4 — CPU samples

200,000 SETs, 8 clients, `-r 100000`. CPU from `top`, second frame.

| server | req/s | avg | min | p50 | p95 | p99 | max | %CPU |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| klyn | 58,928 | 0.099 | 0.024 | 0.087 | 0.175 | 0.279 | 9.551 | 193.8 |
| Redis (run A) | 68,918 | 0.089 | 0.024 | 0.087 | 0.151 | 0.223 | 1.295 | 74.0 |
| Redis (run B) | 71,710 | 0.087 | 0.024 | 0.087 | 0.151 | 0.231 | 1.151 | 73.9 |

Note: Redis's run duration (2.79–2.90 s) is shorter than the two-second `top`
window sampled from t=1 s, so its %CPU is diluted by the tail of the run; the
true in-run figure is nearer 82–94%.

## Discarded

**klyn, c=50, first attempt** — SET 42,265 req/s (p50 0.599, max 2.855),
GET 50,201 req/s (p50 0.511). Discarded: 41% below every later measurement of the
same configuration. Cause never isolated. A second klyn process was later found
alive in another tmux pane, which is the likely explanation.

**Any run taken while a second instance was alive.** Symptom: a `klyn` process at
~99% CPU with zero clients connected, 218 MB RSS, and one socket left in
`CLOSE-WAIT` with 78 bytes unread. After killing it and restarting clean:
0.0% CPU, 15 MB RSS, no lingering sockets.

## Reproduce

```bash
cargo build --release
true > klyn.aof
./target/release/klyn

redis-benchmark -h 127.0.0.1 -p 6379 -n 200000 -c 8 -t set -r 100000
```

```bash
redis-server --port 6380 --save '' --appendonly yes --appendfsync everysec --daemonize yes
redis-cli -p 6380 FLUSHALL
redis-benchmark -h 127.0.0.1 -p 6380 -n 200000 -c 8 -t set -r 100000
redis-cli -p 6380 shutdown nosave
```
