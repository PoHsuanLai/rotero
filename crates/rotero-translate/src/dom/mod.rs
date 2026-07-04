//! DOM + XPath adapter for the JS translator engine (phase 2).
//!
//! Wraps `skyscraper` (HTML parse + XPath) behind a small surface the sandbox's
//! `ZU.xpath`/`ZU.xpathText` host functions call. This is the single place the
//! `<tbody>` and namespace fidelity quirks are handled: skyscraper (like
//! html5ever) inserts an implicit `<tbody>` per the WHATWG spec, which the
//! Zotero translators (written against jsdom + wicked-good-xpath) do not expect.
//! We rewrite `//table/tr` → `//table//tr` so a translator's table XPath still
//! matches.

use skyscraper::xpath;
use skyscraper::xpath::XpathItemTree;
use skyscraper::xpath::grammar::XpathItemTreeNode;
use skyscraper::xpath::grammar::data_model::XpathItem;

/// A parsed HTML document that the engine evaluates XPath against.
pub struct ParsedDom {
    tree: XpathItemTree,
}

impl ParsedDom {
    /// Parse an HTML string into a queryable document.
    pub fn parse(html: &str) -> Result<Self, String> {
        let tree = skyscraper::html::parse(html).map_err(|e| format!("HTML parse error: {e:?}"))?;
        Ok(Self { tree })
    }

    /// Evaluate an XPath expression, returning matched nodes' string values
    /// (attribute values or element text content).
    pub fn xpath_strings(&self, expr: &str) -> Vec<String> {
        let expr = rewrite_table_axis(expr);
        let Ok(xp) = xpath::parse(&expr) else {
            return Vec::new();
        };
        let Ok(items) = xp.apply(&self.tree) else {
            return Vec::new();
        };
        items
            .iter()
            .map(|item| node_string(item, &self.tree))
            .collect()
    }

    /// First matched string value for an XPath expression, or `None`.
    pub fn xpath_text(&self, expr: &str) -> Option<String> {
        self.xpath_strings(expr).into_iter().next()
    }

    /// Number of nodes an XPath expression matches (used by `detect`-style checks).
    pub fn xpath_count(&self, expr: &str) -> usize {
        let expr = rewrite_table_axis(expr);
        xpath::parse(&expr)
            .ok()
            .and_then(|xp| xp.apply(&self.tree).ok())
            .map(|items| items.len())
            .unwrap_or(0)
    }
}

/// Extract a string value from a matched XPath item: the attribute value for
/// attribute nodes, the text content for element nodes, empty otherwise.
fn node_string(item: &XpathItem<'_>, tree: &XpathItemTree) -> String {
    match item {
        XpathItem::Node(node) => match node {
            XpathItemTreeNode::AttributeNode(attr) => attr.value.to_string(),
            XpathItemTreeNode::ElementNode(el) => el.text_content(tree).trim().to_string(),
            XpathItemTreeNode::TextNode(t) => t.content.trim().to_string(),
            _ => String::new(),
        },
        XpathItem::AnyAtomicType(a) => format!("{a:?}"),
        XpathItem::Function(_) => String::new(),
    }
}

/// Rewrite `//table/tr` (and `table/tr` steps) to use the descendant axis
/// `//table//tr`, so the implicit `<tbody>` skyscraper inserts doesn't break a
/// translator's XPath. Conservative: only rewrites the exact `table/tr` step
/// pair, leaving other expressions untouched.
fn rewrite_table_axis(expr: &str) -> String {
    // `table/tr` → `table//tr`, `table/thead` etc. left alone. The descendant
    // axis matches whether or not tbody was inserted.
    expr.replace("table/tr", "table//tr")
        .replace("TABLE/TR", "TABLE//TR")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_attribute_and_text() {
        let dom = ParsedDom::parse(
            r#"<html><head>
                <meta name="citation_title" content="Hello World">
            </head><body><h1>Heading</h1></body></html>"#,
        )
        .unwrap();
        assert_eq!(
            dom.xpath_text(r#"//meta[@name="citation_title"]/@content"#),
            Some("Hello World".to_string())
        );
        assert_eq!(dom.xpath_text("//h1"), Some("Heading".to_string()));
    }

    #[test]
    fn tbody_rewrite_lets_table_xpath_match() {
        // Source has no <tbody>; skyscraper inserts one. Without the rewrite,
        // //table/tr/td matches nothing.
        let dom =
            ParsedDom::parse(r#"<html><body><table><tr><td>cell</td></tr></table></body></html>"#)
                .unwrap();
        // Raw //table/tr/td would be 0; the adapter rewrites it to match.
        assert_eq!(dom.xpath_count("//table/tr/td"), 1);
        assert_eq!(dom.xpath_text("//table/tr/td"), Some("cell".to_string()));
    }

    #[test]
    fn multi_node_xpath() {
        let dom = ParsedDom::parse(
            r#"<html><head>
                <meta name="citation_author" content="A">
                <meta name="citation_author" content="B">
            </head></html>"#,
        )
        .unwrap();
        let vals = dom.xpath_strings(r#"//meta[@name="citation_author"]/@content"#);
        assert_eq!(vals, vec!["A".to_string(), "B".to_string()]);
    }
}
