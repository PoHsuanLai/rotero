//! Font detection and CSS mapping for PDF text extraction.

pub fn detect_font_weight(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("bold") || lower.contains("-bd") || lower.contains("demi") {
        "bold"
    } else if lower.contains("light") || lower.contains("thin") {
        "300"
    } else if lower.contains("black") || lower.contains("heavy") {
        "900"
    } else if lower.contains("medium") && !lower.contains("mediumitalic") {
        "500"
    } else {
        "normal"
    }
}

pub fn detect_font_style(name: &str, is_italic_flag: bool) -> &'static str {
    if is_italic_flag {
        return "italic";
    }
    let lower = name.to_lowercase();
    if lower.contains("italic") || lower.contains("oblique")
        || lower.contains("-it") || lower.contains("slant")
        // LaTeX italic fonts
        || lower.contains("cmti") || lower.contains("cmmi")
    {
        "italic"
    } else {
        "normal"
    }
}

pub fn pdf_font_to_css(name: &str, is_serif: bool) -> String {
    let lower = name.to_lowercase();

    if lower.contains("times") || lower.contains("palatino") || lower.contains("garamond") {
        return format!("\"{name}\", serif");
    }
    if lower.contains("helvetica") || lower.contains("arial") || lower.contains("opensans") {
        return format!("\"{name}\", sans-serif");
    }
    if lower.contains("courier") || lower.contains("consolas") || lower.contains("mono") {
        return format!("\"{name}\", monospace");
    }
    if lower.contains("symbol") || lower.contains("zapf") {
        return format!("\"{name}\", symbol");
    }
    // Computer Modern (LaTeX) — serif
    if lower.contains("cmbx")
        || lower.contains("cmr")
        || lower.contains("cmmi")
        || lower.contains("cmsy")
        || lower.contains("cmex")
        || lower.contains("cmti")
    {
        return format!("\"{name}\", serif");
    }

    if is_serif {
        format!("\"{name}\", serif")
    } else {
        format!("\"{name}\", sans-serif")
    }
}
