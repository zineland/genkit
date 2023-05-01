use std::{fs, path::Path};

use crate::{
    context::Context, current_mode, data, entity::MarkdownConfig, html::rewrite_html_base_url,
    markdown::MarkdownRender, Mode,
};

use anyhow::Result;
use hyper::Uri;
use minijinja::Environment;
use serde_json::Value;

// The `Environment` only includes the fundamental functions and filters.
// It is used for rendering html in some code blocks, such as `QuoteBlock`.
pub fn init_environment<'a>() -> Environment<'a> {
    let mut env = Environment::new();
    env.add_function("markdown_to_html", markdown_to_html_function);
    env.add_function("now", now_function);
    env.add_filter("trim_start_matches", trim_start_matches_filter);
    env.add_function("markdown_to_rss", markdown_to_rss_function);
    env
}

pub fn render(
    env: &Environment,
    template: &str,
    context: Context,
    dest: impl AsRef<Path>,
) -> Result<()> {
    let mut buf = vec![];
    let dest = dest.as_ref().join("index.html");
    if let Some(parent_dir) = dest.parent() {
        if !parent_dir.exists() {
            fs::create_dir_all(parent_dir)?;
        }
    }

    let site = context.get("site").cloned();
    env.get_template(template)?
        .render_to_write(context.into_json(), &mut buf)?;

    // Rewrite some site url and cdn links if and only if:
    // 1. in build run mode
    // 2. site url has a path
    if matches!(current_mode(), Mode::Build) {
        let mut site_url: Option<&str> = None;
        let mut cdn_url: Option<&str> = None;

        if let Some(Value::String(url)) = site.as_ref().and_then(|site| site.get("cdn")) {
            let _ = url.parse::<Uri>().expect("Invalid cdn url.");
            cdn_url = Some(url);
        }
        if let Some(Value::String(url)) = site.as_ref().and_then(|site| site.get("url")) {
            let uri = url.parse::<Uri>().expect("Invalid site url.");
            // We don't need to rewrite links if the site url has a root path.
            if uri.path() != "/" {
                site_url = Some(url);
            }
        }

        let html = rewrite_html_base_url(&buf, site_url, cdn_url)?;
        fs::write(dest, html)?;
        return Ok(());
    }

    fs::write(dest, buf)?;
    Ok(())
}

fn now_function() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("Failed to format now time.")
}

fn trim_start_matches_filter(s: &str, prefix: &str) -> String {
    s.trim_start_matches(prefix).to_string()
}

fn markdown_to_html_function(markdown: &str) -> String {
    let zine_data = data::read();
    let markdown_config = zine_data.get_markdown_config();
    MarkdownRender::new(markdown_config).render_html(markdown)
}

fn markdown_to_rss_function(markdown: &str) -> String {
    let markdown_config = MarkdownConfig {
        highlight_code: false,
        ..Default::default()
    };
    MarkdownRender::new(&markdown_config)
        .enable_rss_mode()
        .render_html(markdown)
}
