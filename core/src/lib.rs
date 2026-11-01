//! Soundboard core: everything that works without Tauri.

pub mod config;

pub mod audio;

pub mod library;

#[cfg(test)]
pub mod test_util;
