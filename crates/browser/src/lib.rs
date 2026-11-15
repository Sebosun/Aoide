//! Renders web pages in a real Chrome or Chromium, for pages plain HTTP cannot read, and opens
//! a visible window for signing in. Nothing here runs unless the app asks for it.

pub mod find;
pub mod patterns;
pub mod ready;
