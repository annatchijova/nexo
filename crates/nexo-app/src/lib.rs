//! Application layer for NEXO: transactions, actor ownership, and durable
//! case state, per `docs/APPLICATION_LAYER_CONTRACT.md`.
//!
//! The schema this layer targets lives in `migrations/0001_init.sql` and is
//! exercised by `scripts/test_schema.sh` against a real PostgreSQL instance.
//! Repository/transaction code (the next slice per the contract's "Status"
//! section) is not implemented yet: this crate currently only declares its
//! dependency boundary so the schema and its contract can be reviewed and
//! tested before any query code is written against it.

pub const SCHEMA_MIGRATION: &str = include_str!("../migrations/0001_init.sql");

pub mod object_store;
