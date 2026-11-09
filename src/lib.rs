//! MCP server for option sellers on Rysk V12.
//!
//! The [`rysk`] module is the functional core: a transport-agnostic client for
//! Rysk's public API and taker websocket (assets, option chains, live quotes,
//! positions, settlement prices). [`mcp`] exposes it as MCP tools. Nothing here
//! holds a key or signs, so nothing here can trade.

pub mod mcp;
pub mod rysk;
