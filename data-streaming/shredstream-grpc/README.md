# shredstream-example

Minimal, self-contained Rust example showing how to subscribe to a
[Shredstream](https://docs.rpcfast.com/rpc-fast-saas-solana/data-streaming/shredstream-grpc) endpoint over gRPC,
deserialize the streamed entries, and apply per-transaction filters.

## Build

```sh
cargo build --release
```

You need a working `protoc` toolchain; this is supplied automatically on
Linux/macOS via the [`protobuf-src`](https://crates.io/crates/protobuf-src)
build dependency.

## Run

```sh
cargo run --release -- --config config.example.yaml
```

Set `RUST_LOG=info` (default) or `RUST_LOG=debug` for more output.

## Configuration

The `--config` flag points at a YAML file. All fields except `endpoint` are
optional. See [`config.example.yaml`](config.example.yaml) for a working
template.

| Field             | Type             | Description                                                                       |
| ----------------- | ---------------- | --------------------------------------------------------------------------------- |
| `endpoint`        | string, required | gRPC URL of the shredstream proxy, e.g. `http://localhost:10100` or `https://...` |
| `x_token`         | string           | Optional auth token, sent as the `x-token` gRPC metadata header                   |
| `account_include` | list of pubkeys  | Keep only transactions touching at least one of these accounts                    |
| `account_exclude` | list of pubkeys  | Drop transactions touching any of these accounts                                  |
| `vote`            | bool             | `true` keeps only vote transactions, `false` drops them, omit to keep both        |
| `buffer_size`     | integer (bytes)  | tonic channel buffer size; default `4194304` (4 MiB)                              |

### Filter semantics

Filters are applied in this order, per transaction:

1. **`vote`** — checks whether any account key equals the Vote program
   (`Vote111111111111111111111111111111111111111`).
2. **`account_exclude`** — drop if any listed account appears.
3. **`account_include`** — keep only if at least one listed account appears.
   Empty list (or omitted) disables the include filter.

Only static account keys from the transaction message are inspected; accounts
referenced via address-lookup tables are not resolved.

### Example: keep pump.fun AMM trades, drop votes

```yaml
endpoint: https://solana-shredstream-grpc.rpcfast.com:443
x_token: "your-token-here"
account_include:
  - pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA
vote: false
```

## Output

For each transaction that passes the filters, the example logs a single line:

```
INFO tx matched slot=303123456 signature=5ab...
```

Use this as a starting point — replace the log call in `main.rs` with whatever
processing your application needs (push to a channel, write to a database,
forward to another service, etc.).

## Layout

```
shredstream-example/
├── Cargo.toml          # project config
├── Cargo.lock
├── build.rs            # compiles protos/shredstream.proto
├── protos/
│   └── shredstream.proto
├── config.example.yaml   # example config
└── src/
    └── main.rs
```
