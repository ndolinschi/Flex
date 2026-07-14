//! HTML cleaning utilities for model-facing page content.

mod content;
mod links;
mod nav;
mod strip;

#[cfg(test)]
mod tests;

pub(crate) use content::clean_html_for_model;
pub use links::extract_page_links;
