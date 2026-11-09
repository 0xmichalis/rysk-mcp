# rysk-mcp

MCP server for option sellers on [Rysk V12](https://rysk.finance): scan the option chain for
covered calls and cash-secured puts, get live maker quotes, and track a wallet's positions and
how they settled. Written in Rust, served over stdio. No credentials and no keys — it cannot
trade, sign, or move funds.

## Tools

| Tool | Description |
|------|-------------|
| `rysk_assets` | Assets per chain with USD price, min/max trade size, and the collateral accepted for calls and puts |
| `rysk_option_chain` | Indicative chain for an underlying (`HYPE`, `BTC`, …): expiries, strikes, delta, seller APY; filter by type, expiry, strike range, min APY, max \|delta\| |
| `rysk_quote` | Live best maker quote for selling an option: premium per contract, total premium, APY, validity |
| `rysk_positions` | A wallet's positions: strike, expiry, quantity, premium, APR, status, and for settled ones whether they were exercised, with the payout or returned collateral |
| `rysk_settlement_prices` | The settlement price of each expiry of an asset |

Amounts are human units: strikes, premiums and prices in USD stablecoins, quantities in the
asset. Payouts keep the exact on-chain `base_units` alongside.

`rysk_quote` sends an anonymous RFQ (the zero address as taker) to Rysk's market makers, as the
web app does while you pick a strike. It does not trade; acting on a quote needs a wallet
signature this server never makes. Rysk only quotes the strikes `rysk_option_chain` lists,
in multiples of the asset's `min_trade_size`.

### Data sources

Rysk documents no public taker API. This server reads the same endpoints the
[Rysk web app](https://app.rysk.finance) uses: `https://v12.rysk.finance/api/*` over HTTP and
the JSON-RPC websocket at `wss://v12.rysk.finance/taker`. Rysk can change them without notice;
`cargo test --test live_rysk -- --ignored` checks them against the live service.

## Install

Prebuilt **static** Linux binaries are attached to each [GitHub Release](../../releases) —
built for `x86_64-unknown-linux-musl`, so they link no libc and run on any x86_64 Linux
regardless of the host's glibc version.

```sh
tar xzf rysk-mcp-vX.Y.Z-x86_64-unknown-linux-musl.tar.gz
install rysk-mcp-vX.Y.Z-x86_64-unknown-linux-musl/rysk-mcp ~/.local/bin/
```

Or build from source: `cargo build --release` (a static musl build uses
[`cross`](https://github.com/cross-rs/cross): `cross build --release --target x86_64-unknown-linux-musl`).

## Setup

Optional: set `RYSK_ADDRESS` to the wallet you sell from, so `rysk_positions` works without an
address argument. Provide it as an environment variable or in a `.env` file in the working
directory (loaded via dotenvy; real environment variables take precedence). A wallet address is
public; no private key is ever needed.

## Use with a Hermes agent

```yaml
mcp_servers:
  rysk:
    command: /home/you/.local/bin/rysk-mcp
    args: []
    env:
      RYSK_ADDRESS: "0xYourWallet"  # optional
    timeout: 120
    connect_timeout: 60
```

Verify: `hermes mcp test rysk` should connect and list the five tools.

## Development

```bash
cargo test                                      # unit + integration (no network)
cargo test --test live_rysk -- --ignored        # live tests; live_quote sends one RFQ
```

Pre-commit hooks (fmt, clippy, check, test) are installed by `cargo-husky` on the first `cargo test`.

## License

MIT
