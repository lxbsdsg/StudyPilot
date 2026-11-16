//! StudyPilot domain, persistence, and platform adapter boundaries.

pub mod adapters;
pub mod api;
pub mod application;
pub mod backend;
pub mod config;
pub mod domain;
pub mod open_targets;
pub mod parse_utils;
pub mod reminder_runner;
pub mod reminders;
pub mod security;
pub mod storage;

pub const VERSION: &str = "0.1.0";
