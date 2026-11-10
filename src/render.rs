use std::borrow::Cow;
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
    count: usize,
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Center,
    Right,
}

struct Table {
    header: Vec<String>,
    align: Vec<Align>,
    rows: Vec<Vec<String>>,
}

pub(crate) struct Renderer {
    markdown: bool,
    color: bool,
    pending: String,
    fence: Option<Fence>,
    table: Option<Table>,
    table_header: Option<(String, String)>,
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
            table: None,
            table_header: None,
            wrote: false,
            ended_with_newline: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn plain() -> Self {
        Self::new(false, false)
    }

    pub(crate) fn will_emit(&self, text: &str) -> bool {
        if !self.markdown {
            return !text.is_empty();
        }

        let Some(last_newline) = text.rfind('\n') else {
            return false;
        };
        let mut table_active = self.table.is_some();
        let mut table_header = self.table_header.is_some();

        for line in text[..last_newline].split('\n') {
            if table_active {
                if is_table_row(line) {
                    continue;
                }
                return true;
            }
            if table_header {
                if delimiter_alignments(line).is_some() {
                    table_active = true;
                    table_header = false;
                    continue;
                }
                return true;
            }
            if is_table_row(line) {
                table_header = true;
                continue;
            }
            return true;
        }

        false
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
            if let Some((indent, header)) = self.table_header.take() {
                self.render_block_line(&indent, &header, out)?;
            }
            self.flush_table(out)?;
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
        let indent_len: usize = line
            .chars()
            .take_while(|&character| character == ' ' || character == '\t')
            .map(char::len_utf8)
            .sum();
        let indent = &line[..indent_len];
        let trimmed = &line[indent_len..];

        if let Some(fence) = &self.fence {
            if fence_close(trimmed, indent, fence) {
                self.write_fence_bottom(out)?;
                self.fence = None;
            } else {
                let content = line.strip_prefix(fence.indent.as_str()).unwrap_or(trimmed);
                out.write_all(fence.indent.as_bytes())?;
                self.write_styled(out, DIM, "│ ")?;
                self.write_styled(out, CYAN, &sanitize(content))?;
                writeln!(out)?;
            }
            return Ok(());
        }

        if let Some((marker, count, info)) = fence_open(trimmed) {
            let info = sanitize(info);
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
                count,
            });
            return Ok(());
        }

        if let Some(table) = self.table.as_mut() {
            if is_table_row(trimmed) {
                table.rows.push(split_row(trimmed));
                return Ok(());
            }
            self.flush_table(out)?;
        }

        if let Some((header_indent, header)) = self.table_header.take() {
            if let Some(align) = delimiter_alignments(trimmed) {
                self.table = Some(Table {
                    header: split_row(&header),
                    align,
                    rows: Vec::new(),
                });
                return Ok(());
            }
            self.render_block_line(&header_indent, &header, out)?;
        }

        if is_table_row(trimmed) {
            self.table_header = Some((indent.to_string(), trimmed.to_string()));
            return Ok(());
        }

        self.render_block_line(indent, trimmed, out)
    }

    fn render_block_line(
        &mut self,
        indent: &str,
        trimmed: &str,
        out: &mut impl Write,
    ) -> io::Result<()> {
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

    fn flush_table(&mut self, out: &mut impl Write) -> io::Result<()> {
        if let Some(table) = self.table.take() {
            self.write_table(&table, out)?;
        }
        Ok(())
    }

    fn write_table(&self, table: &Table, out: &mut impl Write) -> io::Result<()> {
        let columns = table
            .header
            .len()
            .max(table.rows.iter().map(Vec::len).max().unwrap_or(0));
        if columns == 0 {
            return Ok(());
        }

        let align: Vec<Align> = (0..columns)
            .map(|index| table.align.get(index).copied().unwrap_or(Align::Left))
            .collect();
        let header: Vec<String> = (0..columns)
            .map(|index| {
                render_inline(
                    table.header.get(index).map_or("", String::as_str),
                    &[HEADING],
                    self.color,
                )
            })
            .collect();
        let rows: Vec<Vec<String>> = table
            .rows
            .iter()
            .map(|row| {
                (0..columns)
                    .map(|index| {
                        render_inline(row.get(index).map_or("", String::as_str), &[], self.color)
                    })
                    .collect()
            })
            .collect();

        let mut widths = vec![1usize; columns];
        for (index, cell) in header.iter().enumerate() {
            widths[index] = widths[index].max(display_width(cell));
        }
        for row in &rows {
            for (index, cell) in row.iter().enumerate() {
                widths[index] = widths[index].max(display_width(cell));
            }
        }

        let border = |left: &str, middle: &str, right: &str| {
            let segments: Vec<String> = widths.iter().map(|width| "─".repeat(width + 2)).collect();
            format!("{left}{}{right}", segments.join(middle))
        };

        self.write_styled(out, CYAN, &border("┌", "┬", "┐"))?;
        writeln!(out)?;
        self.write_table_row(out, &header, &widths, &align)?;
        if !rows.is_empty() {
            self.write_styled(out, CYAN, &border("├", "┼", "┤"))?;
            writeln!(out)?;
            for row in &rows {
                self.write_table_row(out, row, &widths, &align)?;
            }
        }
        self.write_styled(out, CYAN, &border("└", "┴", "┘"))?;
        writeln!(out)
    }

    fn write_table_row(
        &self,
        out: &mut impl Write,
        cells: &[String],
        widths: &[usize],
        align: &[Align],
    ) -> io::Result<()> {
        self.write_styled(out, CYAN, "│")?;
        for ((cell, width), align) in cells.iter().zip(widths).zip(align) {
            let padding = width.saturating_sub(display_width(cell));
            let (left, right) = match align {
                Align::Left => (0, padding),
                Align::Right => (padding, 0),
                Align::Center => (padding / 2, padding - padding / 2),
            };
            out.write_all(b" ")?;
            for _ in 0..left {
                out.write_all(b" ")?;
            }
            out.write_all(cell.as_bytes())?;
            for _ in 0..right {
                out.write_all(b" ")?;
            }
            self.write_styled(out, CYAN, " │")?;
        }
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

fn fence_open(line: &str) -> Option<(char, usize, &str)> {
    let marker = line.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let count = line.chars().take_while(|&c| c == marker).count();
    if count < 3 {
        return None;
    }
    let info = line[count..].trim();
    if marker == '`' && info.contains('`') {
        return None;
    }
    Some((marker, count, info))
}

fn fence_close(line: &str, indent: &str, fence: &Fence) -> bool {
    let line = line.trim_end();
    let count = line.chars().take_while(|&c| c == fence.marker).count();
    count >= fence.count
        && count == line.chars().count()
        && indent.len() <= fence.indent.len().max(3)
}

pub(crate) fn sanitize(text: &str) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return Cow::Borrowed(text);
    }

    Cow::Owned(
        text.chars()
            .map(|character| match character {
                '\t' => '\t',
                character if character.is_control() => ' ',
                character => character,
            })
            .collect(),
    )
}

fn is_table_row(line: &str) -> bool {
    line.contains('|')
}

fn split_row(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('|').unwrap_or(trimmed);
    let mut cells = Vec::new();
    let mut start = 0;
    let mut escaped = false;

    for (index, character) in trimmed.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '|' => {
                cells.push(trimmed[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    cells.push(trimmed[start..].trim().to_string());
    cells
}

fn delimiter_alignments(line: &str) -> Option<Vec<Align>> {
    let cells = split_row(line);
    if cells.is_empty() {
        return None;
    }

    let mut alignments = Vec::with_capacity(cells.len());
    for cell in cells {
        let cell = cell.trim();
        let left = cell.starts_with(':');
        let right = cell.ends_with(':');
        let dashes = cell.trim_start_matches(':').trim_end_matches(':');
        if dashes.len() < 3 || !dashes.chars().all(|character| character == '-') {
            return None;
        }
        alignments.push(match (left, right) {
            (true, true) => Align::Center,
            (false, true) => Align::Right,
            _ => Align::Left,
        });
    }
    Some(alignments)
}

fn display_width(text: &str) -> usize {
    let mut width = 0;
    let mut escaped = false;
    for character in text.chars() {
        if escaped {
            if character == 'm' {
                escaped = false;
            }
        } else if character == '\x1b' {
            escaped = true;
        } else {
            width += 1;
        }
    }
    width
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
                    inline.leaf(DIM, &format!(" <{}>", sanitize(&url)));
                }
            }
            Event::Code(code) => inline.leaf(YELLOW, &sanitize(&code)),
            Event::Text(text) => {
                let text = sanitize(&text);
                if let Some((_, label)) = &mut link {
                    label.push_str(&text);
                }
                inline.text(&text);
            }
            Event::Html(html) | Event::InlineHtml(html) => inline.text(&sanitize(&html)),
            Event::SoftBreak | Event::HardBreak => inline.text(" "),
            Event::TaskListMarker(checked) => inline.text(checkbox(checked)),
            Event::FootnoteReference(name) => inline.text(&format!("[^{}]", sanitize(&name))),
            Event::InlineMath(math) | Event::DisplayMath(math) => inline.text(&sanitize(&math)),
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
        assert!(!renderer.will_emit("hel"));
        renderer.write("hel", &mut out).unwrap();
        assert!(out.is_empty());
        assert!(renderer.will_emit("lo\n"));
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

        let longer = render("````\ncode\n```\nmore\n````\n");
        assert!(longer.contains("│ ```"));
        assert!(longer.contains("│ more"));
    }

    #[test]
    fn strips_terminal_control_characters() {
        let output = render("hello \u{1b}[31mred\n```\nbody\u{1b}]0;title\u{7}\n```\n");
        assert!(!output.contains('\u{1b}'));
        assert!(!output.contains('\u{7}'));
        assert!(output.contains("hello"));
        assert!(output.contains("body"));
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

        let table =
            render("| Name | Link |\n|:---|---:|\n| Example | [docs](https://example.com) |\n");
        assert!(table.contains("┌"));
        assert!(table.contains("Name"));
        assert!(table.contains("https://example.com"));
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
