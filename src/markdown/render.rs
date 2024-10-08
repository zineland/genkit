use std::{collections::BTreeSet, mem};

use crate::{
    code_blocks::{self, url_preview, CalloutBlock, CodeBlock, Fenced, QuoteBlock},
    entity::MarkdownConfig,
    jinja::init_environment,
};

use minijinja::{context, Environment};
use once_cell::sync::Lazy;
use pulldown_cmark::TagEnd;
use pulldown_cmark::*;
use serde::Serialize;
use syntect::{
    dumps::from_binary, highlighting::ThemeSet, html::highlighted_html_for_string,
    parsing::SyntaxSet,
};

use super::MarkdownVisitor;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(|| {
    let syntax_set: SyntaxSet =
        from_binary(include_bytes!("../../sublime/syntaxes/newlines.packdump"));
    syntax_set
});
static THEME_SET: Lazy<ThemeSet> = Lazy::new(|| {
    let theme_set: ThemeSet = from_binary(include_bytes!("../../sublime/themes/all.themedump"));
    theme_set
});

// Render mode.
enum RenderMode {
    // RSS mode.
    //
    // In RSS mode, we will skip rendering some code blocks, such as `urlpreview`.
    Rss,
    // HTML mode.
    Article,
}

/// Markdown html render.
pub struct MarkdownRender<'a> {
    markdown_env: Environment<'a>,
    markdown_config: &'a MarkdownConfig,
    visitor: Option<Box<dyn MarkdownVisitor + Send + Sync>>,
    code_block_fenced: Option<CowStr<'a>>,
    // Whether we are processing image parsing
    processing_image: bool,
    // The alt of the processing image
    image_alt: Option<CowStr<'a>>,
    // The heading currently being processed.
    curr_heading: Option<Heading<'a>>,
    levels: BTreeSet<usize>,
    render_mode: RenderMode,
    // All headings from markdown, aka, Table of content.
    headings: Option<Vec<Heading<'a>>>,
}

#[derive(Debug, Serialize)]
pub struct Toc {
    // The relative depth.
    depth: usize,
    // Heading level: h1, h2 ... h6
    level: usize,
    // This id is parsed from the markdow heading part.
    // Here is the syntax:
    // `# Long title {#title}` parse the id: title
    // See https://docs.rs/pulldown-cmark/latest/pulldown_cmark/struct.Options.html#associatedconstant.ENABLE_HEADING_ATTRIBUTES
    id: Option<String>,
    // Heading title
    title: String,
}

/// Markdown heading.
#[derive(Debug)]
pub struct Heading<'a> {
    toc: Toc,
    events: Vec<Event<'a>>,
}

impl<'a> Heading<'a> {
    fn new(level: usize, id: Option<String>) -> Self {
        Heading {
            toc: Toc {
                depth: level,
                level,
                id,
                title: String::new(),
            },
            events: Vec::new(),
        }
    }

    fn push_event(&mut self, event: Event<'a>) -> &mut Self {
        self.events.push(event);
        self
    }

    fn push_text(&mut self, text: &str) -> &mut Self {
        self.toc.title.push_str(text);
        self
    }

    // Render heading to html.
    fn render(&mut self, env: &Environment<'a>) -> Event<'static> {
        if self.toc.id.is_none() {
            // Fallback to raw text as the anchor id if the user didn't specify an id.
            self.toc.id = Some(self.toc.title.to_lowercase());
            // Replace blank char with '-'.
            if let Some(id) = self.toc.id.as_mut() {
                *id = id.replace(' ', "-");
            }
        }

        let mut heading = String::new();
        let events = mem::take(&mut self.events);
        html::push_html(&mut heading, events.into_iter());

        let html = env
            .get_template("__genkit_heading.jinja")
            .expect("Get heading template failed.")
            .render(context! {
                heading,
                level => self.toc.level,
                id => self.toc.id,
            })
            .expect("Render heading failed.");
        Event::Html(html.into())
    }
}

impl<'a> MarkdownRender<'a> {
    pub fn new(markdown_config: &'a MarkdownConfig) -> Self {
        MarkdownRender {
            markdown_env: init_environment(),
            markdown_config,
            visitor: None,
            code_block_fenced: None,
            processing_image: false,
            image_alt: None,
            curr_heading: None,
            levels: BTreeSet::new(),
            render_mode: RenderMode::Article,
            headings: None,
        }
    }

    pub fn set_markdown_visitor(
        &mut self,
        visitor: Box<dyn MarkdownVisitor + Send + Sync>,
    ) -> &mut Self {
        self.visitor = Some(visitor);
        self
    }

    /// Enable RSS mode.
    pub fn enable_rss_mode(&mut self) -> &mut Self {
        self.render_mode = RenderMode::Rss;
        self
    }

    /// Enable Table of Content parsing.
    pub fn enable_toc(&mut self) -> &mut Self {
        self.headings = Some(Vec::new());
        self
    }

    /// Get Table of Content list
    pub fn get_toc(&mut self) -> Vec<Toc> {
        if let Some(headings) = mem::take(&mut self.headings) {
            headings.into_iter().map(|h| h.toc).collect()
        } else {
            Vec::new()
        }
    }

    // Rebuild the relative depth of toc items.
    fn rebuild_toc_depth(&mut self) {
        if let Some(headings) = self.headings.as_mut() {
            let depths = Vec::from_iter(&self.levels);
            headings.iter_mut().for_each(|item| {
                item.toc.depth = depths
                    .iter()
                    .position(|&x| *x == item.toc.level)
                    .expect("Invalid heading level")
                    + 1;
            });
        }
    }

    fn highlight_syntax(&self, lang: &str, text: &str) -> String {
        let theme = match THEME_SET.themes.get(&self.markdown_config.highlight_theme) {
            Some(theme) => theme,
            None => panic!(
                "No theme: `{}` founded",
                self.markdown_config.highlight_theme
            ),
        };

        let syntax = SYNTAX_SET
            .find_syntax_by_token(lang)
            // Fallback to plain text if code block not supported
            .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());
        highlighted_html_for_string(text, &SYNTAX_SET, syntax, theme).expect("Highlight failed")
    }

    /// Render markdown to HTML.
    pub fn render_html(&mut self, markdown: &'a str) -> String {
        let parser_events_iter = Parser::new_ext(markdown, Options::all()).into_offset_iter();
        let events = parser_events_iter
            .into_iter()
            .filter_map(|(event, _)| match event {
                Event::Start(tag) => self.visit_start_tag(&tag).resolve(|| Event::Start(tag)),
                Event::End(tag) => self.visit_end_tag(&tag).resolve(|| Event::End(tag)),
                Event::Code(code) => self.visit_code(&code).resolve(|| Event::Code(code)),
                Event::Text(text) => self
                    .visit_text(&text)
                    // Not a code block inside text, or the code block's fenced is unsupported.
                    // We still need record this text event.
                    .resolve(|| Event::Text(text)),
                _ => Some(event),
            });

        let mut html = String::new();
        html::push_html(&mut html, events);
        self.rebuild_toc_depth();
        html
    }

    /// Render code block. Return rendered HTML string if success,
    ///
    /// If the fenced is unsupported, we simply return `None`.
    fn render_code_block(&self, fenced: Fenced, block: &'a str) -> Option<String> {
        match fenced.name {
            code_blocks::URL_PREVIEW => {
                let url = block.trim();
                url_preview::render(url, fenced.options)
            }
            code_blocks::CALLOUT => {
                let html = CalloutBlock::new(fenced.options, block).render().unwrap();
                Some(html)
            }
            code_blocks::QUOTE => {
                let quote = QuoteBlock::parse(block).unwrap();
                let html = self
                    .markdown_env
                    .get_template("__genkit_quote.jinja")
                    .expect("Get quote template failed.")
                    .render(context! {
                        avatar => quote.avatar,
                        author => quote.author,
                        bio => quote.bio,
                        content => quote.content,
                    })
                    .expect("Render quote block failed.");
                Some(html)
            }
            _ => None,
        }
    }

    fn visit_start_tag(&mut self, tag: &Tag<'a>) -> Visiting {
        match tag {
            Tag::CodeBlock(CodeBlockKind::Fenced(name)) => {
                self.code_block_fenced = Some(name.clone());
                Visiting::Ignore
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                let alt = self.image_alt.take().unwrap_or_else(|| CowStr::from(""));
                self.processing_image = false;

                self.processing_image = true;
                // Add loading="lazy" attribute for markdown image.
                Visiting::Event(Event::Html(
                    format!(
                        "<img src=\"{dest_url}\" alt=\"{alt}\" title=\"{title}\" loading=\"lazy\">"
                    )
                    .into(),
                ))
            }
            Tag::Heading { level, id, .. } => {
                self.curr_heading = Some(Heading::new(
                    *level as usize,
                    id.as_ref().map(|i| i.to_string()),
                ));
                Visiting::Ignore
            }
            _ => {
                if let Some(heading) = self.curr_heading.as_mut() {
                    heading.push_event(Event::Start(tag.to_owned()));
                    Visiting::Ignore
                } else {
                    Visiting::NotChanged
                }
            }
        }
    }

    fn visit_end_tag(&mut self, tag: &TagEnd) -> Visiting {
        match tag {
            TagEnd::Image => {
                self.processing_image = false;
                Visiting::Ignore
            }
            TagEnd::CodeBlock => {
                self.code_block_fenced = None;
                Visiting::Ignore
            }
            TagEnd::Heading(..) => {
                if let Some(mut heading) = self.curr_heading.take() {
                    self.levels.insert(heading.toc.level);
                    // Render heading event.
                    let event = heading.render(&self.markdown_env);
                    if let Some(headings) = self.headings.as_mut() {
                        headings.push(heading);
                    }
                    Visiting::Event(event)
                } else {
                    Visiting::Ignore
                }
            }
            _ => {
                if let Some(heading) = self.curr_heading.as_mut() {
                    heading.push_event(Event::End(tag.to_owned()));
                    Visiting::Ignore
                } else {
                    Visiting::NotChanged
                }
            }
        }
    }

    fn visit_text(&mut self, text: &CowStr<'a>) -> Visiting {
        if let Some(heading) = self.curr_heading.as_mut() {
            heading
                .push_text(text.as_ref())
                .push_event(Event::Text(text.to_owned()));
            return Visiting::Ignore;
        }

        if self.processing_image {
            self.image_alt = Some(text.clone());
            return Visiting::Ignore;
        }

        if let Some(input) = self.code_block_fenced.as_ref() {
            let fenced = Fenced::parse(input).unwrap();
            if fenced.name == code_blocks::URL_PREVIEW
                && matches!(self.render_mode, RenderMode::Rss)
            {
                // Ignore url preview in RSS mode.
                return Visiting::Ignore;
            } else if fenced.is_builtin_code_block() {
                let rendered_html = self.render_code_block(fenced, text);
                if let Some(html) = rendered_html {
                    return Visiting::Event(Event::Html(html.into()));
                }
            } else if let Some(html) = self
                .visitor
                .as_ref()
                .and_then(|v| v.visit_custom_block(&fenced, text))
            {
                return Visiting::Event(Event::Html(html.into()));
            } else if self.markdown_config.highlight_code {
                // Syntax highlight
                let html = self.highlight_syntax(fenced.name, text);
                return Visiting::Event(Event::Html(html.into()));
            } else {
                return Visiting::Event(Event::Html(format!("<pre>{}</pre>", text).into()));
            }
        }

        Visiting::NotChanged
    }

    fn visit_code(&mut self, code: &CowStr<'a>) -> Visiting {
        if let Some(heading) = self.curr_heading.as_mut() {
            heading
                .push_text(code.as_ref())
                .push_event(Event::Code(code.to_owned()));
            return Visiting::Ignore;
        }

        if let Some(visitor) = self.visitor.as_ref() {
            if let Some(html) = visitor.visit_code(code) {
                return Visiting::Event(Event::Html(html.into()));
            }
        }

        Visiting::NotChanged
    }
}

/// The markdown visit result.
enum Visiting {
    /// A new event should be rendered.
    Event(Event<'static>),
    /// Nothing changed, still render the origin event.
    NotChanged,
    /// Don't render this event.
    Ignore,
}

impl Visiting {
    fn resolve<'a, F>(self, not_changed: F) -> Option<Event<'a>>
    where
        F: FnOnce() -> Event<'a>,
    {
        match self {
            Visiting::Event(event) => Some(event),
            Visiting::NotChanged => Some(not_changed()),
            Visiting::Ignore => None,
        }
    }
}
