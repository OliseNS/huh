use std::env;
use std::io::{self, IsTerminal, Write};

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

pub(crate) const RESET: &str = "\x1b[0m";
pub(crate) const GREEN: &str = "\x1b[32m";

const BOLD: &str = "\x1b[1m";
pub(crate) const DIM: &str = "\x1b[2m";
const ITALIC: &str = "\x1b[3m";
const UNDERLINE: &str = "\x1b[4m";
const STRIKETHROUGH: &str = "\x1b[9m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[94m";
const HEADING: &str = "\x1b[1m\x1b[36m";

pub(crate) fn color_allowed() -> bool {
    env::var_os("NO_COLOR").is_none() && env::var("TERM").is_ok_and(|term| term != "dumb")
}

struct Fence {
    indent: String,
    marker: char,
}

pub(crate) struct Renderer {
    markdown: bool,
    color: bool,
    pending: String,
    fence: Option<Fence>,
    wrote: bool,
    ended_with_newline: bool,
}

impl Renderer {
    pub(crate) fn stdout() -> Self {
        let markdown = io::stdout().is_terminal();
        Self::new(markdown, markdown && color_allowed())
    }

    fn new(markdown: bool, color: bool) -> Self {
        Self {
            markdown,
            color,
            pending: String::new(),
            fence: None,
            wrote: false,
            ended_with_newline: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn plain() -> Self {
        Self::new(false, false)
    }

    pub(crate) fn write(&mut self, text: &str, out: &mut impl Write) -> io::Result<()> {
        if !self.markdown {
            if !text.is_empty() {
                out.write_all(text.as_bytes())?;
                self.wrote = true;
                self.ended_with_newline = text.ends_with('\n');
            }
            return out.flush();
        }

        self.pending.push_str(text);
        while let Some(position) = self.pending.find('\n') {
            let rest = self.pending.split_off(position + 1);
            let line = std::mem::replace(&mut self.pending, rest);
            self.render_line(line.trim_end_matches(['\r', '\n']), out)?;
        }
        out.flush()
    }

    pub(crate) fn finish(&mut self, out: &mut impl Write) -> io::Result<()> {
        if self.markdown {
            if !self.pending.is_empty() {
                let line = std::mem::take(&mut self.pending);
                self.render_line(line.trim_end_matches(['\r', '\n']), out)?;
            }
            if self.fence.is_some() {
                self.write_fence_bottom(out)?;
                self.fence = None;
            }
        } else if self.wrote && !self.ended_with_newline {
            out.write_all(b"\n")?;
        }
        out.flush()
    }

    fn render_line(&mut self, line: &str, out: &mut impl Write) -> io::Result<()> {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];

        if let Some(fence) = &self.fence {
            if fence_close(trimmed, fence.marker) {
                self.write_fence_bottom(out)?;
                self.fence = None;
            } else {
                let content = line.strip_prefix(fence.indent.as_str()).unwrap_or(trimmed);
                out.write_all(fence.indent.as_bytes())?;
                self.write_styled(out, DIM, "│ ")?;
                self.write_styled(out, CYAN, content)?;
                writeln!(out)?;
            }
            return Ok(());
        }

        if let Some((marker, info)) = fence_open(trimmed) {
            let label = if info.is_empty() {
                String::new()
            } else {
                format!(" {info}")
            };
            out.write_all(indent.as_bytes())?;
            self.write_styled(out, DIM, &format!("╭─{label}"))?;
            writeln!(out)?;
            self.fence = Some(Fence {
                indent: indent.to_string(),
                marker,
            });
            return Ok(());
        }

        if trimmed.is_empty() {
            return writeln!(out);
        }

        if is_rule(trimmed) {
            out.write_all(indent.as_bytes())?;
            self.write_styled(out, DIM, "────────────────────")?;
            return writeln!(out);
        }

        if let Some(text) = heading(trimmed) {
            out.write_all(indent.as_bytes())?;
            return self.write_inline_line(out, text, &[HEADING]);
        }

        if let Some((depth, text)) = blockquote(trimmed) {
            out.write_all(indent.as_bytes())?;
            self.write_styled(out, DIM, &"│ ".repeat(depth))?;
            return self.write_inline_line(out, text, &[DIM, ITALIC]);
        }

        if let Some((marker, text)) = list_item(trimmed) {
            out.write_all(indent.as_bytes())?;
            let (checked, text) = task(text);
            match checked {
                Some(checked) if marker == "•" => {
                    self.write_styled(out, if checked { GREEN } else { DIM }, checkbox(checked))?;
                }
                _ => {
                    self.write_styled(out, CYAN, marker)?;
                    if let Some(checked) = checked {
                        out.write_all(b" ")?;
                        self.write_styled(
                            out,
                            if checked { GREEN } else { DIM },
                            checkbox(checked),
                        )?;
                    }
                }
            }
            out.write_all(b" ")?;
            return self.write_inline_line(out, text, &[]);
        }

        out.write_all(indent.as_bytes())?;
        self.write_inline_line(out, trimmed, &[])
    }

    fn write_inline_line(
        &self,
        out: &mut impl Write,
        text: &str,
        base: &[&'static str],
    ) -> io::Result<()> {
        let rendered = render_inline(text, base, self.color);
        out.write_all(rendered.as_bytes())?;
        writeln!(out)
    }

    fn write_fence_bottom(&self, out: &mut impl Write) -> io::Result<()> {
        if let Some(fence) = &self.fence {
            out.write_all(fence.indent.as_bytes())?;
            self.write_styled(out, DIM, "╰─")?;
            writeln!(out)?;
        }
        Ok(())
    }

    fn write_styled(&self, out: &mut impl Write, style: &str, text: &str) -> io::Result<()> {
        if self.color && !style.is_empty() {
            write!(out, "{style}{text}{RESET}")
        } else {
            out.write_all(text.as_bytes())
        }
    }
}

fn heading(line: &str) -> Option<&str> {
    let level = line
        .chars()
        .take_while(|&character| character == '#')
        .count();
    if level == 0 || level > 6 {
        return None;
    }

    let rest = &line[level..];
    if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    Some(rest.trim_start())
}

fn blockquote(line: &str) -> Option<(usize, &str)> {
    let mut rest = line;
    let mut depth = 0;
    while let Some(after) = rest.strip_prefix('>') {
        rest = after.strip_prefix(' ').unwrap_or(after);
        depth += 1;
    }
    (depth > 0).then_some((depth, rest))
}

fn list_item(line: &str) -> Option<(&str, &str)> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(("•", rest));
        }
    }

    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 {
        let rest = &line[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((&line[..digits + 1], rest));
        }
    }
    None
}

fn task(text: &str) -> (Option<bool>, &str) {
    if let Some(rest) = text.strip_prefix("[ ] ") {
        (Some(false), rest)
    } else if let Some(rest) = text
        .strip_prefix("[x] ")
        .or_else(|| text.strip_prefix("[X] "))
    {
        (Some(true), rest)
    } else {
        (None, text)
    }
}

fn checkbox(checked: bool) -> &'static str {
    if checked { "☑" } else { "☐" }
}

fn is_rule(line: &str) -> bool {
    let compact: String = line
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let mut characters = compact.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    matches!(first, '-' | '*' | '_')
        && compact.chars().count() >= 3
        && characters.all(|c| c == first)
}

fn fence_open(line: &str) -> Option<(char, &str)> {
    let marker = line.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let count = line.chars().take_while(|&c| c == marker).count();
    if count < 3 {
        return None;
    }
    Some((marker, line[count..].trim()))
}

fn fence_close(line: &str, marker: char) -> bool {
    let line = line.trim_end();
    line.chars().count() >= 3 && line.chars().all(|c| c == marker)
}

fn render_inline(text: &str, base: &[&'static str], color: bool) -> String {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;

    let mut inline = Inline::new(base, color, text.len() + 16);
    let mut link: Option<(String, String)> = None;

    for event in Parser::new_ext(text, options) {
        match event {
            Event::Start(Tag::Strong) => inline.push(BOLD),
            Event::End(TagEnd::Strong) => inline.pop(),
            Event::Start(Tag::Emphasis) => inline.push(ITALIC),
            Event::End(TagEnd::Emphasis) => inline.pop(),
            Event::Start(Tag::Strikethrough) => inline.push(STRIKETHROUGH),
            Event::End(TagEnd::Strikethrough) => inline.pop(),
            Event::Start(Tag::Link { dest_url, .. }) => {
                link = Some((dest_url.to_string(), String::new()));
                inline.push(UNDERLINE);
                inline.push(BLUE);
            }
            Event::End(TagEnd::Link) => {
                inline.pop();
                inline.pop();
                if let Some((url, label)) = link.take()
                    && label != url
                {
                    inline.leaf(DIM, &format!(" <{url}>"));
                }
            }
            Event::Code(code) => inline.leaf(YELLOW, &code),
            Event::Text(text) => {
                if let Some((_, label)) = &mut link {
                    label.push_str(&text);
                }
                inline.text(&text);
            }
            Event::Html(html) | Event::InlineHtml(html) => inline.text(&html),
            Event::SoftBreak | Event::HardBreak => inline.text(" "),
            Event::TaskListMarker(checked) => inline.text(checkbox(checked)),
            Event::FootnoteReference(name) => inline.text(&format!("[^{name}]")),
            Event::InlineMath(math) | Event::DisplayMath(math) => inline.text(&math),
            Event::Rule => inline.text("───"),
            Event::Start(_) | Event::End(_) => {}
        }
    }

    inline.finish()
}

struct Inline {
    color: bool,
    styles: Vec<&'static str>,
    output: String,
}

impl Inline {
    fn new(base: &[&'static str], color: bool, capacity: usize) -> Self {
        let mut inline = Self {
            color,
            styles: base.to_vec(),
            output: String::with_capacity(capacity),
        };
        if color {
            for style in &inline.styles {
                inline.output.push_str(style);
            }
        }
        inline
    }

    fn text(&mut self, text: &str) {
        self.output.push_str(text);
    }

    fn push(&mut self, style: &'static str) {
        self.styles.push(style);
        if self.color {
            self.output.push_str(style);
        }
    }

    fn pop(&mut self) {
        self.styles.pop();
        self.reapply();
    }

    fn leaf(&mut self, style: &'static str, text: &str) {
        if self.color {
            self.output.push_str(RESET);
            self.output.push_str(style);
        }
        self.output.push_str(text);
        self.reapply();
    }

    fn reapply(&mut self) {
        if !self.color {
            return;
        }
        self.output.push_str(RESET);
        for style in &self.styles {
            self.output.push_str(style);
        }
    }

    fn finish(mut self) -> String {
        if self.color {
            self.output.push_str(RESET);
        }
        self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(input: &str) -> String {
        let mut renderer = Renderer::new(true, false);
        let mut out = Vec::new();
        renderer.write(input, &mut out).unwrap();
        renderer.finish(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn raw_mode_passes_text_through() {
        let mut renderer = Renderer::new(false, false);
        let mut out = Vec::new();
        renderer.write("**raw**", &mut out).unwrap();
        renderer.finish(&mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "**raw**\n");
    }

    #[test]
    fn streams_complete_lines_only() {
        let mut renderer = Renderer::new(true, false);
        let mut out = Vec::new();
        renderer.write("hel", &mut out).unwrap();
        assert!(out.is_empty());
        renderer.write("lo\nworld", &mut out).unwrap();
        assert_eq!(String::from_utf8(out.clone()).unwrap(), "hello\n");
        renderer.finish(&mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "hello\nworld\n");
    }

    #[test]
    fn renders_code_fences() {
        let output = render("```rust\nlet x = 1;\n```\n");
        assert!(output.contains("╭─ rust"));
        assert!(output.contains("│ let x = 1;"));
        assert!(output.contains("╰─"));
        assert!(!output.contains("```"));
    }

    #[test]
    fn fences_only_close_with_their_opening_marker() {
        let info = render("```\n``` rust\ncode\n```\n");
        assert!(info.contains("│ ``` rust"));
        assert!(info.contains("│ code"));

        let tildes = render("```\ncode\n~~~\nmore\n```\n");
        assert!(tildes.contains("│ ~~~"));
        assert!(tildes.contains("│ more"));
    }

    #[test]
    fn renders_common_markdown() {
        let headings = render("# Title\n- **bold** item\n");
        assert!(headings.contains("Title"));
        assert!(!headings.contains('#'));
        assert!(headings.contains("• bold item"));
        assert!(!headings.contains("**"));

        let links = render("see [docs](https://example.com) and `code`\n");
        assert!(links.contains("docs <https://example.com>"));
        assert!(links.contains("code"));
        assert!(!links.contains("`code`"));

        assert!(render("- [x] done\n").contains("☑ done"));
        assert!(render("- [ ] todo\n").contains("☐ todo"));
    }

    #[test]
    fn adds_color_when_enabled() {
        let mut renderer = Renderer::new(true, true);
        let mut out = Vec::new();
        renderer.write("# Colored\n", &mut out).unwrap();
        renderer.finish(&mut out).unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains(HEADING));
        assert!(output.ends_with(RESET) || output.contains(RESET));
    }
}
