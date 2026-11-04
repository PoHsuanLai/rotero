use pulldown_cmark::{Event, Options, Parser, Tag, html};

fn latex_to_mathml(latex: &str, display: bool) -> String {
    let storage = pulldown_latex::Storage::new();
    let parser = pulldown_latex::Parser::new(latex, &storage);
    let mut mathml = String::new();
    let config = pulldown_latex::RenderConfig {
        display_mode: if display {
            pulldown_latex::config::DisplayMode::Block
        } else {
            pulldown_latex::config::DisplayMode::Inline
        },
        ..Default::default()
    };
    match pulldown_latex::push_mathml(&mut mathml, parser, config) {
        Ok(()) => mathml,
        Err(_) => {
            // Fall back to showing the raw LaTeX in a code element
            let tag = if display { "div" } else { "span" };
            format!(
                "<{tag} class=\"latex-error\">{}</{tag}>",
                html_escape(latex)
            )
        }
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn md_to_html(text: &str) -> String {
    render_markdown(text, false)
}

/// Like [`md_to_html`], for text that agents write or peers sync in.
///
/// Raw HTML in the source is shown as text, and links and images whose
/// destination is not http(s), mailto, or a relative path lose their target.
pub fn md_to_html_untrusted(text: &str) -> String {
    render_markdown(text, true)
}

fn is_safe_url(url: &str) -> bool {
    let cleaned: String = url
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    match cleaned.split_once(':') {
        None => true,
        Some((scheme, _)) => {
            // A colon after a path or query separator is not a scheme.
            scheme.contains(['/', '?', '#']) || matches!(scheme, "http" | "https" | "mailto")
        }
    }
}

fn render_markdown(text: &str, untrusted: bool) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_MATH);

    let parser = Parser::new_ext(text, options);

    // Process math events into raw HTML, pass everything else through
    let events: Vec<Event<'_>> = parser
        .map(|event| match event {
            Event::InlineMath(latex) => Event::InlineHtml(latex_to_mathml(&latex, false).into()),
            Event::DisplayMath(latex) => Event::Html(latex_to_mathml(&latex, true).into()),
            Event::Html(raw) | Event::InlineHtml(raw) if untrusted => Event::Text(raw),
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) if untrusted && !is_safe_url(&dest_url) => Event::Start(Tag::Link {
                link_type,
                dest_url: "".into(),
                title,
                id,
            }),
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) if untrusted && !is_safe_url(&dest_url) => Event::Start(Tag::Image {
                link_type,
                dest_url: "".into(),
                title,
                id,
            }),
            other => other,
        })
        .collect();

    let mut html_output = String::new();
    html::push_html(&mut html_output, events.into_iter());
    html_output
}

/// Render plain text that may contain LaTeX (e.g. paper abstracts).
/// Converts `$...$` and `$$...$$` to MathML, escapes the rest as HTML.
pub fn text_with_latex(text: &str) -> String {
    // Use the markdown pipeline — it handles math and produces safe HTML.
    // Wrapping in a paragraph is fine since pulldown-cmark does that anyway.
    md_to_html(text)
}
