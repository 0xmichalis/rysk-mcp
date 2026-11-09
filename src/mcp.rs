//! MCP server exposing the Rysk tools over stdio (via `rmcp`).
//!
//! Five tools for an option seller: `rysk_assets`, `rysk_option_chain`,
//! `rysk_quote`, `rysk_positions` and `rysk_settlement_prices`. None of them
//! signs anything, so none can trade or move funds.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData, ServerHandler, tool, tool_handler, tool_router};
use serde::Serialize;

use crate::rysk::transport::NetTransport;
use crate::rysk::{ChainFilter, OptionType, PositionFilter, QuoteRequest, RyskClient, RyskError};

/// The MCP server: a Rysk client over the production transport, plus the wallet
/// `rysk_positions` reads when the caller names none.
pub struct RyskServer {
    client: RyskClient<NetTransport>,
    default_address: Option<String>,
}

impl RyskServer {
    pub fn new(client: RyskClient<NetTransport>, default_address: Option<String>) -> Self {
        Self {
            client,
            default_address,
        }
    }
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AssetsArgs {
    /// Only assets on this chain: 999 (HyperEVM), 1 (Ethereum) or 8453 (Base).
    pub chain_id: Option<u64>,
    /// Only assets of this underlying, e.g. "HYPE" or "BTC".
    pub underlying: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OptionChainArgs {
    /// Underlying symbol ("HYPE", "BTC", "ETH") or a tradable asset symbol ("WHYPE").
    pub underlying: String,
    /// "call" (covered call) or "put" (cash-secured put). Omit for both.
    pub option_type: Option<OptionType>,
    /// Only this expiry date, YYYY-MM-DD (UTC).
    pub expiry: Option<String>,
    pub min_strike: Option<f64>,
    pub max_strike: Option<f64>,
    /// Only strikes whose indicative APY (percent) is at least this.
    pub min_apy: Option<f64>,
    /// Only strikes whose absolute delta is at most this, e.g. 0.2.
    pub max_abs_delta: Option<f64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QuoteArgs {
    /// Asset symbol ("WHYPE", "UBTC", "WETH") or address. See rysk_assets.
    pub asset: String,
    /// Chain of the asset, needed when its symbol exists on several chains.
    pub chain_id: Option<u64>,
    /// "call" to sell a covered call, "put" to sell a cash-secured put.
    pub option_type: OptionType,
    /// Strike in USD, e.g. 75 or 0.85. Must be a strike rysk_option_chain lists.
    pub strike: f64,
    /// Expiry date YYYY-MM-DD (UTC), or the expiry_timestamp from rysk_option_chain.
    pub expiry: String,
    /// Number of contracts in units of the asset (e.g. 25 WHYPE): a multiple of the
    /// asset's min_trade_size, at most its max_trade_size.
    pub quantity: f64,
    /// Collateral symbol or address. Defaults to the first one rysk_assets lists.
    pub collateral: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PositionsArgs {
    /// Wallet address. Defaults to RYSK_ADDRESS from the server's environment.
    pub address: Option<String>,
    /// Only positions with this status, e.g. "SETTLED".
    pub status: Option<String>,
    /// Only positions on this chain.
    pub chain_id: Option<u64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SettlementPricesArgs {
    /// Asset symbol ("WHYPE", "UBTC") or address.
    pub asset: String,
    /// Chain of the asset, needed when its symbol exists on several chains.
    pub chain_id: Option<u64>,
}

#[tool_router]
impl RyskServer {
    #[tool(
        name = "rysk_assets",
        description = "List the assets Rysk V12 lets you sell options on, per chain: USD \
                       price, min/max trade size, and the collateral accepted for covered \
                       calls and cash-secured puts. Start here to find asset symbols.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn assets(
        &self,
        Parameters(args): Parameters<AssetsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let result = self
            .client
            .assets(args.chain_id, args.underlying.as_deref())
            .await
            .map(|assets| serde_json::json!({ "assets": assets }));
        Ok(to_result(result))
    }

    #[tool(
        name = "rysk_option_chain",
        description = "Fetch Rysk's indicative option chain for an underlying (e.g. HYPE, \
                       BTC): expiries, strikes, delta and the seller's indicative APY, sorted \
                       by expiry then strike. Filter by type, expiry, strike range, minimum \
                       APY or maximum |delta|. APYs are indicative; use rysk_quote for a \
                       live price.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn option_chain(
        &self,
        Parameters(args): Parameters<OptionChainArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let filter = ChainFilter {
            underlying: args.underlying,
            option_type: args.option_type,
            expiry: args.expiry,
            min_strike: args.min_strike,
            max_strike: args.max_strike,
            min_apy: args.min_apy,
            max_abs_delta: args.max_abs_delta,
        };
        Ok(to_result(self.client.option_chain(&filter).await))
    }

    #[tool(
        name = "rysk_quote",
        description = "Get a live maker quote for selling an option on Rysk: premium per \
                       contract, total premium, APY and how long the quote is valid. This \
                       sends an anonymous RFQ to Rysk's market makers; it does not trade, \
                       sign or lock collateral.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = true
        )
    )]
    async fn quote(
        &self,
        Parameters(args): Parameters<QuoteArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let request = QuoteRequest {
            asset: args.asset,
            chain_id: args.chain_id,
            option_type: args.option_type,
            strike: args.strike,
            expiry: args.expiry,
            quantity: args.quantity,
            collateral: args.collateral,
        };
        Ok(to_result(self.client.quote(&request).await))
    }

    #[tool(
        name = "rysk_positions",
        description = "List a wallet's Rysk option positions, latest expiry first: asset, \
                       call/put, strike, expiry, quantity, premium earned, APR at entry and \
                       status. Settled positions say whether they were exercised, with the \
                       payout or the collateral returned.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn positions(
        &self,
        Parameters(args): Parameters<PositionsArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let Some(address) = args.address.or_else(|| self.default_address.clone()) else {
            return Ok(failure(
                "pass an address, or set RYSK_ADDRESS for the server".to_string(),
            ));
        };
        let filter = PositionFilter {
            status: args.status,
            chain_id: args.chain_id,
        };
        let result = self
            .client
            .positions(&address, &filter)
            .await
            .map(|positions| {
                serde_json::json!({
                    "address": address.trim().to_lowercase(),
                    "positions": positions,
                })
            });
        Ok(to_result(result))
    }

    #[tool(
        name = "rysk_settlement_prices",
        description = "List the settlement price Rysk used at each expiry of an asset, latest \
                       first. Unsettled expiries have a null price. Compare with a position's \
                       strike to see how it settled.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn settlement_prices(
        &self,
        Parameters(args): Parameters<SettlementPricesArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(to_result(
            self.client
                .settlement_prices(&args.asset, args.chain_id)
                .await,
        ))
    }
}

#[tool_handler]
impl ServerHandler for RyskServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Rysk V12 (rysk.finance) for an option seller: covered calls and \
                 cash-secured puts for upfront premium. Typical flow: `rysk_assets` for \
                 symbols, trade sizes and collateral; `rysk_option_chain` to scan strikes and \
                 indicative APYs; `rysk_quote` for a live maker price; `rysk_positions` to \
                 track a wallet; `rysk_settlement_prices` to check how expiries settled. \
                 Prices and premiums are in USD stablecoins. Nothing here can trade.",
            )
    }
}

/// Map a client outcome to an MCP tool result. A failure is a tool-level error
/// (`is_error = true`) so the model sees the message, rather than a protocol error.
fn to_result<T: Serialize>(result: Result<T, RyskError>) -> CallToolResult {
    match result.map(|value| serde_json::to_value(value)) {
        Ok(Ok(value)) => CallToolResult::structured(value),
        Ok(Err(err)) => failure(format!("serializing the result failed: {err}")),
        Err(err) => failure(err.to_string()),
    }
}

fn failure(message: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_map_to_structured_json() {
        let result = to_result(Ok(serde_json::json!({ "assets": [{ "symbol": "WHYPE" }] })));

        assert_ne!(result.is_error, Some(true));
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("structuredContent"), "json: {json}");
        assert!(json.contains("\"symbol\":\"WHYPE\""), "json: {json}");
    }

    #[test]
    fn client_errors_map_to_tool_errors_with_message() {
        let result = to_result::<()>(Err(RyskError::Api("invalid strike (code -32603)".into())));

        assert_eq!(result.is_error, Some(true));
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("invalid strike"), "json: {json}");
    }

    #[test]
    fn option_type_accepts_common_spellings() {
        for (text, expected) in [
            ("\"call\"", OptionType::Call),
            ("\"CALL\"", OptionType::Call),
            ("\"C\"", OptionType::Call),
            ("\"put\"", OptionType::Put),
            ("\"Put\"", OptionType::Put),
            ("\"p\"", OptionType::Put),
        ] {
            assert_eq!(serde_json::from_str::<OptionType>(text).unwrap(), expected);
        }
        assert!(serde_json::from_str::<OptionType>("\"straddle\"").is_err());
    }
}
