use crate::{
    entity::MarkdownConfig,
    markdown::{self, MarkdownRender},
};

use minijinja::Environment;

// The `Environment` only includes the fundamental functions and filters.
// It is used for rendering html in some code blocks, such as `QuoteBlock`.
pub fn init_environment<'a>() -> Environment<'a> {
    let mut env = Environment::new();
    let templates = [
        (
            "__genkit_heading.jinja",
            include_str!("../templates/heading.jinja"),
        ),
        (
            "__genkit_quote.jinja",
            include_str!("../templates/quote.jinja"),
        ),
    ];
    for (name, template) in templates {
        env.add_template(name, template).unwrap();
    }

    env.add_function("markdown_to_html", markdown_to_html_function);
    env.add_function("now", now_function);
    env.add_filter("trim_start_matches", trim_start_matches_filter);
    env.add_function("markdown_to_rss", markdown_to_rss_function);
    env
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
    markdown::render_html(markdown)
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
