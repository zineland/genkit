use std::{borrow::Cow, io::Read};

use html5ever::{
    parse_document, tendril::TendrilSink, tree_builder::TreeBuilderOpts, Attribute, ParseOpts,
};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

use serde::Serialize;

/// The meta info of the HTML page.
#[derive(Debug, Default, Serialize)]
pub struct Meta<'a> {
    pub title: Cow<'a, str>,
    pub description: Cow<'a, str>,
    pub url: Option<Cow<'a, str>>,
    pub image: Option<Cow<'a, str>>,
}

impl<'a> Meta<'a> {
    fn truncate(&mut self) {
        self.title.to_mut().truncate(200);
        self.description.to_mut().truncate(200);
    }
}

/// Parse HTML [`Meta`] from `html`.
pub fn parse_html_meta<'a, R: Read>(mut html: R) -> Meta<'a> {
    let parse_opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            scripting_enabled: false,
            drop_doctype: true,
            ..Default::default()
        },
        ..Default::default()
    };

    let rc_dom = parse_document(RcDom::default(), parse_opts)
        .from_utf8()
        .read_from(&mut html)
        .unwrap();

    let mut meta = Meta::default();
    if let NodeData::Document = rc_dom.document.data {
        let children = rc_dom.document.children.borrow();
        for child in children.iter() {
            if walk(child, &mut meta, "html") {
                // Stop traverse.
                break;
            }
        }
    } else {
        walk(&rc_dom.document, &mut meta, "html");
    }
    meta.truncate();
    meta
}

// Walk html tree to parse [`Meta`].
// `super_node` is the current node we traversing in.
//
// Return true if we should stop traversing.
fn walk(handle: &Handle, meta: &mut Meta, super_node: &str) -> bool {
    fn get_attribute<'a>(attrs: &'a [Attribute], name: &'a str) -> Option<&'a str> {
        attrs.iter().find_map(|attr| {
            if attr.name.local.as_ref() == name {
                let value = attr.value.as_ref().trim();
                // Some value of attribute is empty, such as:
                // <meta property="og:title" content="" />
                if value.is_empty() {
                    None
                } else {
                    Some(value)
                }
            } else {
                None
            }
        })
    }

    if let NodeData::Element {
        ref name,
        ref attrs,
        ..
    } = &handle.data
    {
        match name.local.as_ref() {
            node_name @ ("html" | "head") => {
                let children = handle.children.borrow();
                for child in children.iter() {
                    if walk(child, meta, node_name) {
                        // Stop traverse.
                        return true;
                    }
                }
            }
            "meta" if super_node == "head" => {
                // <meta name="description" content="xxx"/>
                // get description value from attribute.
                let attrs = &*attrs.borrow();
                match get_attribute(attrs, "name").or_else(|| get_attribute(attrs, "property")) {
                    Some("description" | "og:description" | "twitter:description")
                        if meta.description.is_empty() =>
                    {
                        if let Some(description) = get_attribute(attrs, "content") {
                            meta.description = Cow::Owned(description.trim().to_owned());
                        }
                    }
                    Some("og:title" | "twitter:title") if meta.title.is_empty() => {
                        if let Some(title) = get_attribute(attrs, "content") {
                            meta.title = Cow::Owned(title.trim().to_owned());
                        }
                    }
                    Some("og:image" | "twitter:image") if meta.image.is_none() => {
                        if let Some(image) = get_attribute(attrs, "content") {
                            meta.image = Some(Cow::Owned(image.to_owned()));
                        }
                    }
                    // url
                    Some("og:url" | "twitter:url") if meta.url.is_none() => {
                        if let Some(url) = get_attribute(attrs, "content") {
                            meta.url = Some(Cow::Owned(url.to_owned()));
                        }
                    }
                    _ => {}
                }
            }
            "link" if super_node == "head" => {
                // TODO: Extract favicon from <link> tag
            }
            "title" if super_node == "head" => {
                // Extract <title> tag.
                // Some title tag may have multiple empty text child nodes,
                // we need handle this case:
                //   <title>
                //
                //       Rust Programming Language
                //
                //   </title>
                let title = handle
                    .children
                    .borrow()
                    .iter()
                    .filter_map(|h| match &h.data {
                        NodeData::Text { contents } => {
                            let contents = contents.borrow();
                            Some(contents.to_string())
                        }
                        _ => None,
                    })
                    .collect::<String>();
                meta.title = Cow::Owned(title.trim().to_owned());
            }
            _ => {}
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::parse_html_meta;

    #[test]
    fn test_parse_html_meta1() {
        let html = r#"
<!DOCTYPE html><html lang="en" class="notranslate" translate="no">
<head>
<meta charset="utf-8">
<meta http-equiv="X-UA-Compatible" content="IE=edge">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>crates.io: Rust Package Registry</title>
<link rel="shortcut icon" href="/favicon.ico" type="image/x-icon">
<link rel="icon" href="/assets/cargo.png" type="image/png">
<meta name="google" content="notranslate">
<meta property="og:image" content="/assets/og-image.png">
<meta name="twitter:card" content="summary_large_image">
</head>
<body></body></html>
        "#;
        let meta = parse_html_meta(html.as_bytes());
        assert_eq!(meta.title, "crates.io: Rust Package Registry");
        assert_eq!(meta.description, "");
        assert_eq!(meta.url, None);
        assert_eq!(meta.image, Some("/assets/og-image.png".into()));
    }

    #[test]
    fn test_parse_html_meta2() {
        let html = r#"
        
<!DOCTYPE html><html lang="en" class="notranslate" translate="no"><head>
<meta charset="utf-8">
<meta http-equiv="X-UA-Compatible" content="IE=edge">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>crates.io: Rust Package Registry</title>
<link rel="shortcut icon" href="/favicon.ico" type="image/x-icon">
<link rel="icon" href="/assets/cargo.png" type="image/png">
<link rel="search" href="/opensearch.xml" type="application/opensearchdescription+xml" title="Cargo">

<meta property="og:image" content="/assets/og-image.png">
<meta name="twitter:card" content="summary_large_image">

<body>
</body></html>
        "#;
        let meta = parse_html_meta(html.as_bytes());
        assert_eq!(meta.title, "crates.io: Rust Package Registry");
        assert_eq!(meta.description, "",);
        assert_eq!(meta.url, None);
        assert_eq!(meta.image, Some("/assets/og-image.png".into()));
    }

    #[test]
    fn test_parse_html_meta3() {
        let html = r#"<!DOCTYPE html><html lang="en" class="notranslate" translate="no">
<head>
<meta charset="utf-8">
<meta http-equiv="X-UA-Compatible" content="IE=edge">
<meta name="viewport" content="width=device-width, initial-scale=1">
<link rel="shortcut icon" href="/favicon.ico" type="image/x-icon">
<link rel="icon" href="/assets/cargo.png" type="image/png">
<meta property="og:image" content="/assets/og-image.png">
<meta name="twitter:card" content="summary_large_image">
<title>crates.io: Rust Package Registry</title>
<meta name="description" content="crates.io is a Rust community effort to create a shared registry of crates.">

<meta property="og:url" content="https://crates.io/">
<meta name="twitter:url" content="https://crates.io/">

</head>

<body></body>
<footer>
<title>fake title</title>
</footer>
</html>
        "#;
        let meta = parse_html_meta(html.as_bytes());
        assert_eq!(meta.title, "crates.io: Rust Package Registry");
        assert_eq!(
            meta.description,
            "crates.io is a Rust community effort to create a shared registry of crates."
        );
        assert_eq!(meta.url, Some("https://crates.io/".into()));
        assert_eq!(meta.image, Some("/assets/og-image.png".into()));
    }

    #[test]
    fn test_parse_html_meta4() {
        let html = r#"<head>
        <meta charset="utf-8">
        <meta http-equiv="X-UA-Compatible" content="IE=edge">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <link rel="shortcut icon" href="/favicon.ico" type="image/x-icon">
        <link rel="icon" href="/assets/cargo.png" type="image/png">
        <meta property="og:image" content="/assets/og-image.png">
        <meta name="twitter:card" content="summary_large_image">
        <title>crates.io: Rust Package Registry</title>
        <meta name="description" content="crates.io is a Rust community effort to create a shared registry of crates.">
        
        <meta property="og:url" content="https://crates.io/">
        <meta name="twitter:url" content="https://crates.io/">
        
        </head>"#;
        let meta = parse_html_meta(html.as_bytes());
        assert_eq!(meta.title, "crates.io: Rust Package Registry");
        assert_eq!(
            meta.description,
            "crates.io is a Rust community effort to create a shared registry of crates."
        );
        assert_eq!(meta.url, Some("https://crates.io/".into()));
        assert_eq!(meta.image, Some("/assets/og-image.png".into()));
    }
}
