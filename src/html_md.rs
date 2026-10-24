//! Converts an HTML page to Markdown with the standard library only, so the site build can compile it alone.

/// Elements whose content is not page text.
const SKIP: &[&str] = &[
    "script", "style", "svg", "noscript", "template", "nav", "footer", "button", "select",
    "textarea", "iframe", "canvas",
];
const BLOCK: &[&str] = &[
    "p",
    "div",
    "section",
    "article",
    "main",
    "header",
    "aside",
    "form",
    "figure",
    "figcaption",
    "details",
    "summary",
    "blockquote",
    "dl",
    "dt",
    "dd",
    "ul",
    "ol",
    "table",
];

struct Out {
    text: String,
    pre: bool,
}

impl Out {
    fn text(&mut self, s: &str) {
        for c in s.chars() {
            if self.pre {
                self.text.push(c);
            } else if c.is_whitespace() {
                if !self.text.is_empty() && !self.text.ends_with([' ', '\n']) {
                    self.text.push(' ');
                }
            } else {
                self.text.push(c);
            }
        }
    }

    fn lines(&mut self, n: usize) {
        let kept = self.text.trim_end_matches(' ').len();
        self.text.truncate(kept);
        if self.text.is_empty() {
            return;
        }
        while self.text.chars().rev().take_while(|c| *c == '\n').count() < n {
            self.text.push('\n');
        }
    }
}

fn entity(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').filter(|e| *e <= 8).and_then(|e| {
            let name = &rest[1..e];
            let c = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => ' ',
                _ => {
                    let n = name.strip_prefix('#')?;
                    let code = match n.strip_prefix(['x', 'X']) {
                        Some(h) => u32::from_str_radix(h, 16).ok()?,
                        None => n.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            };
            Some((c, e + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let at = lower.find(&format!("{name}="))? + name.len() + 1;
    let rest = &tag[at..];
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let end = rest[1..].find(quote)?;
    Some(entity(&rest[1..1 + end]))
}

/// Converts an HTML page to Markdown: headings, paragraphs, lists, links, emphasis, code and tables.
pub fn convert(html: &str) -> String {
    let mut out = Out {
        text: String::new(),
        pre: false,
    };
    let mut title = String::new();
    let mut in_title = false;
    let mut skip: Option<(String, usize)> = None;
    let mut links: Vec<(usize, String)> = Vec::new();
    let mut lists: Vec<bool> = Vec::new();
    let mut saw_h1 = false;
    let mut rest = html;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            if skip.is_none() && !in_title {
                out.text(&entity(rest));
            }
            break;
        };
        let (before, tagged) = rest.split_at(lt);
        if !before.is_empty() {
            if in_title {
                title.push_str(&entity(before));
            } else if skip.is_none() {
                out.text(&entity(before));
            }
        }
        if let Some(body) = tagged.strip_prefix("<!--") {
            rest = body.find("-->").map_or("", |e| &body[e + 3..]);
            continue;
        }
        let mut end = None;
        let mut quote = None;
        for (i, c) in tagged.char_indices().skip(1) {
            match (quote, c) {
                (None, '"' | '\'') => quote = Some(c),
                (Some(q), c) if c == q => quote = None,
                (None, '>') => {
                    end = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            break;
        };
        let tag = &tagged[1..end];
        rest = &tagged[end + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if let Some((skipped, depth)) = &mut skip {
            if name == *skipped {
                if closing {
                    *depth -= 1;
                    if *depth == 0 {
                        skip = None;
                    }
                } else if !tag.ends_with('/') {
                    *depth += 1;
                }
            }
            continue;
        }
        if name == "title" {
            in_title = !closing;
            continue;
        }
        if !closing && SKIP.contains(&name.as_str()) && !tag.ends_with('/') {
            skip = Some((name, 1));
            continue;
        }
        match (name.as_str(), closing) {
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                out.lines(2);
                let level = (name.as_bytes()[1] - b'0') as usize;
                saw_h1 |= level == 1;
                out.text.push_str(&"#".repeat(level));
                out.text.push(' ');
            }
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => out.lines(2),
            ("br", _) => out.lines(1),
            ("hr", _) => {
                out.lines(2);
                out.text.push_str("---");
                out.lines(2);
            }
            ("ul", false) => {
                out.lines(if lists.is_empty() { 2 } else { 1 });
                lists.push(false);
            }
            ("ol", false) => {
                out.lines(if lists.is_empty() { 2 } else { 1 });
                lists.push(true);
            }
            ("ul" | "ol", true) => {
                lists.pop();
                out.lines(if lists.is_empty() { 2 } else { 1 });
            }
            ("li", false) => {
                out.lines(1);
                out.text
                    .push_str(&"  ".repeat(lists.len().saturating_sub(1)));
                out.text.push_str(if lists.last() == Some(&true) {
                    "1. "
                } else {
                    "- "
                });
            }
            ("li", true) => out.lines(1),
            ("tr", true) => out.lines(1),
            ("td" | "th", true) => out.text.push_str(" | "),
            ("pre", false) => {
                out.lines(2);
                out.text.push_str("```\n");
                out.pre = true;
            }
            ("pre", true) => {
                out.pre = false;
                out.lines(1);
                out.text.push_str("```");
                out.lines(2);
            }
            ("code", _) if !out.pre => out.text.push('`'),
            ("strong" | "b", _) => out.text.push_str("**"),
            ("em", _) => out.text.push('*'),
            ("a", false) => {
                let href = attr(tag, "href").unwrap_or_default();
                links.push((out.text.len(), href));
            }
            ("a", true) => {
                if let Some((at, href)) = links.pop() {
                    let label = out.text[at..].trim().to_string();
                    if href.is_empty() || href.starts_with(['#']) || href.starts_with("javascript")
                    {
                        continue;
                    }
                    out.text.truncate(at);
                    if !label.is_empty() {
                        out.text.push_str(&format!("[{label}]({href})"));
                    }
                }
            }
            (n, _) if BLOCK.contains(&n) => out.lines(if n == "p" { 2 } else { 1 }),
            _ => {}
        }
    }
    let body = out.text.replace("****", "").trim().to_string();
    let title = title.trim();
    if !saw_h1 && !title.is_empty() {
        format!("# {title}\n\n{body}\n")
    } else {
        format!("{body}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_a_page() {
        let html = "<html><head><title>Pricing &amp; plans</title><style>p{}</style></head><body>\
            <nav><a href=\"/x\">Menu</a></nav><h2>Plans</h2><p>Self-host is <strong>free</strong>. \
            See <a href=\"/docs\">the docs</a>.</p><ul><li>One</li><li>Two<ul><li>Nested</li></ul></li></ul>\
            <pre>a &lt; b\n  c</pre><script>alert(1)</script><footer>Legal</footer></body></html>";
        let md = convert(html);
        assert_eq!(
            md,
            "# Pricing & plans\n\n## Plans\n\nSelf-host is **free**. See [the docs](/docs).\n\n- One\n- Two\n  - Nested\n\n```\na < b\n  c\n```\n"
        );
    }
}
