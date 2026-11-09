//! The functional core: a transport-agnostic client for the parts of the Rysk V12
//! API an option seller needs.
//!
//! Public HTTP endpoints serve assets, products, the indicative option chain and
//! settlement prices. The taker websocket answers JSON-RPC calls for a wallet's
//! positions and for live RFQ quotes. Nothing here signs or trades: an RFQ only
//! asks makers for a price, and acting on it would need a signed confirmation.

mod api;
pub mod model;
pub mod transport;
pub mod units;

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde_json::json;

use api::{
    Assets, Inventory, RawAsset, RawExpiryPrice, RawPosition, RawProduct, RawQuote, RpcResponse,
};
pub use model::{
    Asset, OptionChain, OptionRow, OptionType, Position, ProductRef, Quote, SettlementPrice,
    SettlementPrices, TokenAmount,
};
use transport::Transport;
use units::{PRICE_DECIMALS, WAD, from_base_units, parse_utc_date, to_base_units, utc_date};

/// Base URL of the public HTTP API.
pub const API_URL: &str = "https://v12.rysk.finance";
/// JSON-RPC websocket the Rysk web app uses for takers.
pub const TAKER_WS_URL: &str = "wss://v12.rysk.finance/taker";

/// RFQs go out without a wallet, so they name the zero address as taker.
const ANONYMOUS_TAKER: &str = "0x0000000000000000000000000000000000000000";

/// Strike and premium stablecoins per chain, as the Rysk web app lists them. They
/// are not in `/api/assets`; all of them have 6 decimals on-chain.
const STABLECOINS: &[(u64, &str, &str)] = &[
    (1, "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC"),
    (1, "0xdac17f958d2ee523a2206206994597c13d831ec7", "USDT"),
    (999, "0xb88339cb7199b77e23db6e890353e22632ba630f", "USDC"),
    (999, "0xb8ce59fc3717ada4c02eadf9682a9e934f625ebb", "USDT0"),
    (999, "0x111111a1a0667d36bd57c0a9f569b98057111111", "USDH"),
    (8453, "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913", "USDC"),
];
const STABLECOIN_DECIMALS: u32 = 6;

/// Errors from a Rysk query.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RyskError {
    /// The caller's input does not match anything Rysk offers.
    #[error("{0}")]
    Invalid(String),
    /// Rysk answered with an error.
    #[error("Rysk rejected the request: {0}")]
    Api(String),
    /// Rysk answered with something this client cannot read.
    #[error("unexpected Rysk response: {0}")]
    Parse(String),
    #[error("{0}")]
    Transport(String),
}

/// Filters for [`RyskClient::option_chain`].
#[derive(Debug, Clone, Default)]
pub struct ChainFilter {
    /// Underlying symbol (`HYPE`, `BTC`) or a tradable asset symbol (`WHYPE`).
    pub underlying: String,
    pub option_type: Option<OptionType>,
    /// Expiry date, `YYYY-MM-DD` (UTC).
    pub expiry: Option<String>,
    pub min_strike: Option<f64>,
    pub max_strike: Option<f64>,
    pub min_apy: Option<f64>,
    pub max_abs_delta: Option<f64>,
}

/// The option to price with [`RyskClient::quote`].
#[derive(Debug, Clone)]
pub struct QuoteRequest {
    /// Asset symbol (`WHYPE`, `UBTC`) or address.
    pub asset: String,
    pub chain_id: Option<u64>,
    pub option_type: OptionType,
    pub strike: f64,
    /// Expiry as `YYYY-MM-DD` (UTC) or a unix timestamp in seconds.
    pub expiry: String,
    /// Contracts, in units of the asset.
    pub quantity: f64,
    /// Collateral symbol or address; defaults to the first one Rysk lists.
    pub collateral: Option<String>,
}

/// Filters for [`RyskClient::positions`].
#[derive(Debug, Clone, Default)]
pub struct PositionFilter {
    /// Exact status, case-insensitive (Rysk reports e.g. `SETTLED`).
    pub status: Option<String>,
    pub chain_id: Option<u64>,
}

/// Runs Rysk queries over an injected [`Transport`].
pub struct RyskClient<T: Transport> {
    transport: T,
    api_url: String,
    ws_url: String,
}

impl<T: Transport> RyskClient<T> {
    /// Create a client against the production endpoints.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            api_url: API_URL.to_string(),
            ws_url: TAKER_WS_URL.to_string(),
        }
    }

    /// Override the endpoints (used by tests to point at local servers).
    pub fn with_urls(mut self, api_url: impl Into<String>, ws_url: impl Into<String>) -> Self {
        self.api_url = api_url.into();
        self.ws_url = ws_url.into();
        self
    }

    /// List tradable assets with their USD price, trade size limits and the
    /// collateral each can be sold against, sorted by chain then symbol.
    pub async fn assets(
        &self,
        chain_id: Option<u64>,
        underlying: Option<&str>,
    ) -> Result<Vec<Asset>, RyskError> {
        let (assets, products) = tokio::try_join!(self.fetch_assets(), self.fetch_products())?;
        let tokens = Tokens::new(&assets);

        let mut rows: Vec<Asset> = assets
            .iter()
            .filter(|a| chain_id.is_none_or(|c| a.chain_id == c))
            .filter(|a| underlying.is_none_or(|u| a.underlying.eq_ignore_ascii_case(u.trim())))
            .map(|a| {
                let collateral = |is_put: bool| -> Vec<String> {
                    products_for(&products, a, is_put)
                        .into_iter()
                        .map(|p| tokens.symbol(a.chain_id, &p.collateral))
                        .collect()
                };
                Asset {
                    chain_id: a.chain_id,
                    chain: chain_name(a.chain_id),
                    symbol: a.symbol.clone(),
                    underlying: a.underlying.clone(),
                    address: a.address.to_lowercase(),
                    decimals: a.decimals,
                    price_usd: from_base_units(&a.price, WAD),
                    min_trade_size: from_base_units(&a.min_trade_size, WAD),
                    max_trade_size: from_base_units(&a.max_trade_size, WAD),
                    active: a.active,
                    call_collateral: collateral(false),
                    put_collateral: collateral(true),
                }
            })
            .collect();
        rows.sort_by(|a, b| (a.chain_id, &a.symbol).cmp(&(b.chain_id, &b.symbol)));
        Ok(rows)
    }

    /// Fetch the indicative option chain of an underlying, sorted by expiry, then
    /// strike, then calls before puts.
    pub async fn option_chain(&self, filter: &ChainFilter) -> Result<OptionChain, RyskError> {
        let expiry_day = match filter.expiry.as_deref() {
            Some(date) => Some(parse_utc_date(date).ok_or_else(|| {
                RyskError::Invalid(format!("expiry must be YYYY-MM-DD, got {date:?}"))
            })?),
            None => None,
        };
        let (inventory, assets) = tokio::try_join!(
            self.get_json::<Inventory>("/api/inventory"),
            self.fetch_assets()
        )?;
        let underlying = resolve_underlying(&inventory, &assets, &filter.underlying)?;
        let tokens = Tokens::new(&assets);
        let combinations = inventory[&underlying]
            .combinations
            .clone()
            .unwrap_or_default();

        let index_price = combinations.values().map(|c| c.index).next();
        let as_of = combinations.values().map(|c| c.timestamp).max();
        let mut options: Vec<OptionRow> = combinations
            .into_values()
            .filter(|c| filter.option_type.is_none_or(|t| t.is_put() == c.is_put))
            .filter(|c| {
                expiry_day
                    .is_none_or(|day| c.expiration_timestamp.div_euclid(86_400) * 86_400 == day)
            })
            .filter(|c| filter.min_strike.is_none_or(|m| c.strike >= m))
            .filter(|c| filter.max_strike.is_none_or(|m| c.strike <= m))
            .filter(|c| filter.min_apy.is_none_or(|m| c.apy >= m))
            .filter(|c| filter.max_abs_delta.is_none_or(|m| c.delta.abs() <= m))
            .map(|c| OptionRow {
                expiry: utc_date(c.expiration_timestamp),
                expiry_timestamp: c.expiration_timestamp,
                days_to_expiry: round(c.time_to_expiry_days, 2),
                option_type: OptionType::from_is_put(c.is_put),
                strike: c.strike,
                delta: round(c.delta, 4),
                apy: round(c.apy, 2),
                bid: (c.bid != 0.0).then_some(c.bid),
                ask: (c.ask != 0.0).then_some(c.ask),
                products: c
                    .products
                    .iter()
                    .map(|p| {
                        let chain_id = tokens.chain_of(&p.asset);
                        ProductRef {
                            chain_id,
                            asset: tokens.symbol_on(chain_id, &p.asset),
                            collateral: tokens.symbol_on(chain_id, &p.collateral_asset),
                        }
                    })
                    .collect(),
            })
            .collect();
        options.sort_by(|a, b| {
            a.expiry_timestamp
                .cmp(&b.expiry_timestamp)
                .then(a.strike.total_cmp(&b.strike))
                .then(a.option_type.is_put().cmp(&b.option_type.is_put()))
        });

        Ok(OptionChain {
            underlying,
            index_price,
            as_of: as_of.map(units::utc_datetime),
            options,
        })
    }

    /// Ask Rysk's makers for a live price to sell an option. This starts an RFQ
    /// auction and returns the best quote; it does not trade.
    pub async fn quote(&self, request: &QuoteRequest) -> Result<Quote, RyskError> {
        let strike = to_base_units(request.strike, PRICE_DECIMALS)
            .map_err(|e| RyskError::Invalid(format!("strike {e}")))?;
        let quantity = to_base_units(request.quantity, WAD)
            .map_err(|e| RyskError::Invalid(format!("quantity {e}")))?;
        let (assets, products) = tokio::try_join!(self.fetch_assets(), self.fetch_products())?;
        let tokens = Tokens::new(&assets);
        let asset = resolve_asset(&assets, &request.asset, request.chain_id)?;
        let product = pick_product(&products, &tokens, asset, request)?;
        let expiry = self.resolve_expiry(asset, &request.expiry).await?;

        let params = json!({
            "asset": asset.address.to_lowercase(),
            "chainId": asset.chain_id,
            "expiry": expiry,
            "isPut": request.option_type.is_put(),
            "isTakerBuy": false,
            "quantity": quantity,
            "strike": strike,
            "taker": ANONYMOUS_TAKER,
            "usd": product.usd.to_lowercase(),
            "collateralAsset": product.collateral.to_lowercase(),
        });
        let quote: RawQuote = self.rpc("request", params).await?;

        let premium_asset = quote
            .premium_asset
            .as_deref()
            .filter(|a| !a.is_empty())
            .unwrap_or(&quote.usd);
        let price = from_base_units(&quote.price, WAD);
        let quantity = from_base_units(&quote.quantity, WAD);
        Ok(Quote {
            asset: asset.symbol.clone(),
            chain_id: asset.chain_id,
            option_type: request.option_type,
            strike: from_base_units(&quote.strike, PRICE_DECIMALS),
            expiry: utc_date(quote.expiry),
            expiry_timestamp: quote.expiry,
            quantity,
            price_per_contract: price,
            total_premium: price.zip(quantity).map(|(p, q)| round(p * q, 6)),
            premium_asset: tokens.symbol(asset.chain_id, premium_asset),
            apy: quote.apy.map(|a| round(a, 2)),
            collateral: tokens.symbol(asset.chain_id, &quote.collateral_asset),
            maker: quote.maker.to_lowercase(),
            valid_until: units::utc_datetime(quote.valid_until),
        })
    }

    /// List the option positions of a wallet, latest expiry first.
    pub async fn positions(
        &self,
        address: &str,
        filter: &PositionFilter,
    ) -> Result<Vec<Position>, RyskError> {
        let address = address.trim().to_lowercase();
        if !is_address(&address) {
            return Err(RyskError::Invalid(format!(
                "address must be a 0x-prefixed 20-byte hex address, got {address:?}"
            )));
        }
        let (assets, raw) = tokio::try_join!(
            self.fetch_assets(),
            self.rpc::<Vec<RawPosition>>("positions", json!({ "address": address }))
        )?;
        let tokens = Tokens::new(&assets);

        let mut positions: Vec<Position> = raw
            .into_iter()
            .filter(|p| filter.chain_id.is_none_or(|c| p.chain_id == c))
            .filter(|p| {
                filter
                    .status
                    .as_deref()
                    .is_none_or(|s| p.status.eq_ignore_ascii_case(s.trim()))
            })
            .map(|p| position(p, &tokens))
            .collect();
        positions.sort_by(|a, b| {
            b.expiry_timestamp
                .cmp(&a.expiry_timestamp)
                .then_with(|| b.created_at.cmp(&a.created_at))
        });
        Ok(positions)
    }

    /// Fetch the settlement price of every expiry of an asset, latest first.
    pub async fn settlement_prices(
        &self,
        asset: &str,
        chain_id: Option<u64>,
    ) -> Result<SettlementPrices, RyskError> {
        let assets = self.fetch_assets().await?;
        let asset = resolve_asset(&assets, asset, chain_id)?;
        let path = format!(
            "/api/expiry-prices/{}/{}",
            asset.chain_id,
            asset.address.to_lowercase()
        );
        let raw: HashMap<String, RawExpiryPrice> = self.get_json(&path).await?;

        let mut prices = raw
            .into_iter()
            .map(|(expiry, price)| {
                let expiry_timestamp: i64 = expiry
                    .parse()
                    .map_err(|_| RyskError::Parse(format!("{path}: expiry {expiry:?}")))?;
                let (raw_price, verified) = match price {
                    RawExpiryPrice::Plain(price) => (Some(price), None),
                    RawExpiryPrice::Detailed { price, verified } => (price, Some(verified)),
                };
                Ok(SettlementPrice {
                    expiry: utc_date(expiry_timestamp),
                    expiry_timestamp,
                    price: raw_price
                        .and_then(|p| from_base_units(&p, PRICE_DECIMALS))
                        .filter(|p| *p != 0.0),
                    verified,
                })
            })
            .collect::<Result<Vec<_>, RyskError>>()?;
        prices.sort_by_key(|p| std::cmp::Reverse(p.expiry_timestamp));

        Ok(SettlementPrices {
            asset: asset.symbol.clone(),
            chain_id: asset.chain_id,
            prices,
        })
    }

    /// Turn a `YYYY-MM-DD` expiry into the exact timestamp Rysk lists for that
    /// day; pass a unix timestamp through.
    async fn resolve_expiry(&self, asset: &RawAsset, expiry: &str) -> Result<i64, RyskError> {
        let expiry = expiry.trim();
        if let Ok(timestamp) = expiry.parse::<i64>() {
            return Ok(timestamp);
        }
        let day = parse_utc_date(expiry).ok_or_else(|| {
            RyskError::Invalid(format!(
                "expiry must be YYYY-MM-DD or a unix timestamp, got {expiry:?}"
            ))
        })?;
        let inventory: Inventory = self.get_json("/api/inventory").await?;
        let expiries = inventory
            .get(&asset.underlying)
            .and_then(|u| u.expiries.clone())
            .unwrap_or_default();
        expiries
            .iter()
            .copied()
            .find(|e| e.div_euclid(86_400) * 86_400 == day)
            .ok_or_else(|| {
                let listed: Vec<String> = expiries.iter().map(|e| utc_date(*e)).collect();
                RyskError::Invalid(format!(
                    "no {} expiry on {expiry}; listed expiries: {}",
                    asset.underlying,
                    list_or_none(&listed)
                ))
            })
    }

    async fn fetch_assets(&self) -> Result<Vec<RawAsset>, RyskError> {
        let assets: Assets = self.get_json("/api/assets").await?;
        Ok(assets.into_values().flatten().collect())
    }

    async fn fetch_products(&self) -> Result<Vec<RawProduct>, RyskError> {
        self.get_json("/api/products").await
    }

    async fn get_json<D: DeserializeOwned>(&self, path: &str) -> Result<D, RyskError> {
        let url = format!("{}{path}", self.api_url);
        let body = self
            .transport
            .get(&url)
            .await
            .map_err(|e| RyskError::Transport(format!("GET {path}: {e}")))?;
        serde_json::from_str(&body).map_err(|e| RyskError::Parse(format!("{path}: {e}")))
    }

    async fn rpc<D: DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<D, RyskError> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": method,
            "method": method,
            "params": params,
        })
        .to_string();
        let body = self
            .transport
            .rpc(&self.ws_url, method, &request)
            .await
            .map_err(|e| RyskError::Transport(format!("{method}: {e}")))?;
        let response: RpcResponse<D> =
            serde_json::from_str(&body).map_err(|e| RyskError::Parse(format!("{method}: {e}")))?;
        if let Some(error) = response.error {
            return Err(RyskError::Api(format!(
                "{} (code {})",
                error.message, error.code
            )));
        }
        response
            .result
            .ok_or_else(|| RyskError::Parse(format!("{method}: neither result nor error")))
    }
}

/// Symbols and decimals of the tokens Rysk deals in, keyed by chain and address.
struct Tokens {
    by_address: HashMap<(u64, String), (String, u32)>,
}

impl Tokens {
    fn new(assets: &[RawAsset]) -> Self {
        let stablecoins = STABLECOINS.iter().map(|(chain, address, symbol)| {
            (
                (*chain, address.to_string()),
                (symbol.to_string(), STABLECOIN_DECIMALS),
            )
        });
        let listed = assets.iter().map(|a| {
            (
                (a.chain_id, a.address.to_lowercase()),
                (a.symbol.clone(), a.decimals),
            )
        });
        Self {
            by_address: stablecoins.chain(listed).collect(),
        }
    }

    fn get(&self, chain_id: u64, address: &str) -> Option<&(String, u32)> {
        self.by_address.get(&(chain_id, address.to_lowercase()))
    }

    /// The token's symbol, or its lowercase address when unknown.
    fn symbol(&self, chain_id: u64, address: &str) -> String {
        self.get(chain_id, address)
            .map(|(symbol, _)| symbol.clone())
            .unwrap_or_else(|| address.to_lowercase())
    }

    fn symbol_on(&self, chain_id: Option<u64>, address: &str) -> String {
        match chain_id {
            Some(chain_id) => self.symbol(chain_id, address),
            None => address.to_lowercase(),
        }
    }

    /// The chain a token address is listed on. Inventory products carry no chain id.
    fn chain_of(&self, address: &str) -> Option<u64> {
        let address = address.to_lowercase();
        self.by_address
            .keys()
            .filter(|(_, a)| *a == address)
            .map(|(chain, _)| *chain)
            .min()
    }

    fn amount(&self, chain_id: u64, address: &str, base_units: String) -> TokenAmount {
        let decimals = self.get(chain_id, address).map(|(_, d)| *d);
        TokenAmount {
            asset: self.symbol(chain_id, address),
            amount: decimals.and_then(|d| from_base_units(&base_units, d)),
            base_units,
        }
    }
}

fn position(p: RawPosition, tokens: &Tokens) -> Position {
    let nonzero = |raw: &Option<String>| {
        raw.clone()
            .filter(|v| !v.trim_start_matches('0').is_empty())
    };
    let payout = nonzero(&p.payout);
    let collateral_returned = nonzero(&p.settlement_collateral_payout);
    let payout_asset = p.payout_asset.clone().unwrap_or_else(|| p.usd.clone());
    let symbol = if p.symbol.is_empty() {
        tokens.symbol(p.chain_id, &p.address)
    } else {
        p.symbol.clone()
    };
    let tx_hash = p.tx_hash.filter(|h| !h.is_empty());

    Position {
        chain_id: p.chain_id,
        symbol,
        option_type: OptionType::from_is_put(p.is_put),
        exercised: p
            .status
            .eq_ignore_ascii_case("SETTLED")
            .then_some(payout.is_some()),
        status: p.status,
        strike: from_base_units(&p.strike, WAD),
        expiry: utc_date(p.expiry),
        expiry_timestamp: p.expiry,
        quantity: from_base_units(&p.quantity, WAD),
        price_per_contract: from_base_units(&p.price, WAD),
        premium: from_base_units(&p.premium, WAD),
        premium_asset: tokens.symbol(p.chain_id, &p.usd),
        apr: p.apr.and_then(|a| a.parse().ok()),
        collateral: tokens.symbol(p.chain_id, &p.collateral),
        created_at: units::utc_datetime(p.created_at),
        payout: payout.map(|units| tokens.amount(p.chain_id, &payout_asset, units)),
        collateral_returned: collateral_returned
            .map(|units| tokens.amount(p.chain_id, &p.collateral, units)),
        tx_url: tx_hash
            .as_deref()
            .and_then(|hash| explorer_tx_url(p.chain_id, hash)),
        tx_hash,
    }
}

/// Active products that sell `asset` as a call or a put, in Rysk's order.
fn products_for<'p>(
    products: &'p [RawProduct],
    asset: &RawAsset,
    is_put: bool,
) -> Vec<&'p RawProduct> {
    products
        .iter()
        .filter(|p| {
            p.active
                && p.is_put == is_put
                && p.chain_id == asset.chain_id
                && p.asset_address.eq_ignore_ascii_case(&asset.address)
        })
        .collect()
}

fn pick_product<'a>(
    products: &'a [RawProduct],
    tokens: &Tokens,
    asset: &RawAsset,
    request: &QuoteRequest,
) -> Result<&'a RawProduct, RyskError> {
    let candidates = products_for(products, asset, request.option_type.is_put());
    let collateral_symbols: Vec<String> = candidates
        .iter()
        .map(|p| tokens.symbol(asset.chain_id, &p.collateral))
        .collect();
    let kind = request.option_type.as_str();
    let Some(wanted) = request.collateral.as_deref().map(str::trim) else {
        return candidates.first().copied().ok_or_else(|| {
            RyskError::Invalid(format!(
                "Rysk offers no {kind} on {} (chain {})",
                asset.symbol, asset.chain_id
            ))
        });
    };
    candidates
        .iter()
        .zip(&collateral_symbols)
        .find(|(p, symbol)| {
            symbol.eq_ignore_ascii_case(wanted) || p.collateral.eq_ignore_ascii_case(wanted)
        })
        .map(|(p, _)| *p)
        .ok_or_else(|| {
            RyskError::Invalid(format!(
                "{} {kind}s cannot use {wanted} as collateral; accepted: {}",
                asset.symbol,
                list_or_none(&collateral_symbols)
            ))
        })
}

/// Find one asset by address, symbol, or underlying symbol, optionally on one chain.
fn resolve_asset<'a>(
    assets: &'a [RawAsset],
    query: &str,
    chain_id: Option<u64>,
) -> Result<&'a RawAsset, RyskError> {
    let query = query.trim();
    let find = |matches: &dyn Fn(&RawAsset) -> bool| -> Vec<&'a RawAsset> {
        let mut found: Vec<&RawAsset> = assets
            .iter()
            .filter(|a| chain_id.is_none_or(|c| a.chain_id == c) && matches(a))
            .collect();
        found.sort_by(|a, b| (a.chain_id, &a.symbol).cmp(&(b.chain_id, &b.symbol)));
        found.dedup_by(|a, b| {
            a.chain_id == b.chain_id && a.address.eq_ignore_ascii_case(&b.address)
        });
        found
    };
    let found = if query.starts_with("0x") {
        find(&|a| a.address.eq_ignore_ascii_case(query))
    } else {
        let by_symbol = find(&|a| a.symbol.eq_ignore_ascii_case(query));
        if by_symbol.is_empty() {
            find(&|a| a.underlying.eq_ignore_ascii_case(query))
        } else {
            by_symbol
        }
    };
    let on_chain = chain_id
        .map(|c| format!(" on chain {c}"))
        .unwrap_or_default();

    match found.as_slice() {
        [asset] => Ok(asset),
        [] => {
            let known: Vec<String> = find(&|a| a.active).iter().map(|a| label(a)).collect();
            Err(RyskError::Invalid(format!(
                "unknown asset {query:?}{on_chain}; active assets: {}",
                list_or_none(&known)
            )))
        }
        several => {
            let labels: Vec<String> = several.iter().map(|a| label(a)).collect();
            Err(RyskError::Invalid(format!(
                "{query:?}{on_chain} matches several assets: {}; pass one symbol and its chain_id",
                labels.join(", ")
            )))
        }
    }
}

/// Map an underlying or asset symbol to an inventory key.
fn resolve_underlying(
    inventory: &Inventory,
    assets: &[RawAsset],
    query: &str,
) -> Result<String, RyskError> {
    let query = query.trim();
    let key_for = |name: &str| {
        inventory
            .keys()
            .find(|k| k.eq_ignore_ascii_case(name))
            .cloned()
    };
    key_for(query)
        .or_else(|| {
            assets
                .iter()
                .filter(|a| a.symbol.eq_ignore_ascii_case(query))
                .find_map(|a| key_for(&a.underlying))
        })
        .ok_or_else(|| {
            let known: Vec<String> = inventory.keys().cloned().collect();
            RyskError::Invalid(format!(
                "unknown underlying {query:?}; Rysk lists: {}",
                list_or_none(&known)
            ))
        })
}

fn label(asset: &RawAsset) -> String {
    format!("{} (chain {})", asset.symbol, asset.chain_id)
}

fn list_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

fn is_address(text: &str) -> bool {
    text.len() == 42 && text.starts_with("0x") && text[2..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn round(value: f64, places: i32) -> f64 {
    let factor = 10f64.powi(places);
    (value * factor).round() / factor
}

fn chain_name(chain_id: u64) -> Option<&'static str> {
    match chain_id {
        1 => Some("Ethereum"),
        999 => Some("HyperEVM"),
        8453 => Some("Base"),
        _ => None,
    }
}

fn explorer_tx_url(chain_id: u64, hash: &str) -> Option<String> {
    let base = match chain_id {
        1 => "https://etherscan.io/tx/",
        999 => "https://hyperevmscan.io/tx/",
        8453 => "https://basescan.org/tx/",
        _ => return None,
    };
    Some(format!("{base}{hash}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_validated() {
        assert!(is_address("0x418e14e4d34add3c23d2be1c11e6b02e628ae6dd"));
        assert!(!is_address("0x418e14e4d34add3c23d2be1c11e6b02e628ae6d"));
        assert!(!is_address("418e14e4d34add3c23d2be1c11e6b02e628ae6dd00"));
        assert!(!is_address("0x418e14e4d34add3c23d2be1c11e6b02e628ae6zz"));
    }

    #[test]
    fn round_keeps_requested_places() {
        assert_eq!(round(142.43613245614674, 2), 142.44);
        assert_eq!(round(-0.06282, 4), -0.0628);
    }
}
