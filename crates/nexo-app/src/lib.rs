//! Application layer for NEXO: transactions, actor ownership, and durable
//! case state, per `docs/APPLICATION_LAYER_CONTRACT.md`.
//!
//! The schema this layer targets lives in `migrations/0001_init.sql` and is
//! exercised by `scripts/test_schema.sh` against a real PostgreSQL instance.
//! `repository` implements the transaction contract against that schema.

pub const SCHEMA_MIGRATION: &str = include_str!("../migrations/0001_init.sql");
pub const AUDIT_CHAIN_MIGRATION: &str = include_str!("../migrations/0002_audit_chain_v1.sql");

pub mod audit;
pub mod export;
pub mod object_store;
pub mod repository;
