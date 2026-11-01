//! Soundboard core: everything that works without Tauri.

pub mod config;

pub mod audio;

pub mod library;

pub mod hotkeys;

#[cfg(test)]
pub mod test_util;
