use std::borrow::Cow;
use std::env;
use std::io::{self, IsTerminal, Write};

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use unicode_width::UnicodeWidthChar;

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
    indent: String,
    header: Vec<String>,
    align: Vec<Align>,
    rows: Vec<Vec<String>>,
}

struct Cell {
    runs: Vec<Run>,
    natural: usize,
    longest_word: usize,
}

impl Cell {
    fn new(text: &str, base: &[&'static str]) -> Self {
        let text = text.replace('\t', "    ");
        let runs = inline_runs(&text, base);
        let natural = runs_width(&runs).max(1);
        let longest_word = words(&runs)
            .iter()
            .map(|word| runs_width(word))
            .max()
            .unwrap_or(1)
            .max(1);
        Self {
            runs,
            natural,
            longest_word,
        }
    }
}

pub(crate) struct Renderer {
    markdown: bool,
    color: bool,
    width: usize,
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
        let mut renderer = Self::new(markdown, markdown && color_allowed());
        renderer.width = 0;
        renderer
    }

    #[cfg(test)]
    fn with_width(markdown: bool, color: bool, width: usize) -> Self {
        let mut renderer = Self::new(markdown, color);
        renderer.width = width;
        renderer
    }

    fn new(markdown: bool, color: bool) -> Self {
        Self {
            markdown,
            color,
            width: 80,
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
                    indent: header_indent,
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
        let runs = inline_runs(text, base);
        self.write_runs(out, &runs)?;
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
        let header: Vec<Cell> = (0..columns)
            .map(|index| {
                Cell::new(
                    table.header.get(index).map_or("", String::as_str),
                    &[HEADING],
                )
            })
            .collect();
        let rows: Vec<Vec<Cell>> = table
            .rows
            .iter()
            .map(|row| {
                (0..columns)
                    .map(|index| Cell::new(row.get(index).map_or("", String::as_str), &[]))
                    .collect()
            })
            .collect();

        let natural: Vec<usize> = (0..columns)
            .map(|index| {
                rows.iter()
                    .map(|row| row[index].natural)
                    .max()
                    .unwrap_or(0)
                    .max(header[index].natural)
                    .max(1)
            })
            .collect();
        let minima: Vec<usize> = (0..columns)
            .map(|index| {
                rows.iter()
                    .map(|row| row[index].longest_word)
                    .max()
                    .unwrap_or(0)
                    .max(header[index].longest_word)
                    .max(1)
            })
            .collect();

        let indent = &table.indent;
        let available = self
            .terminal_width()
            .saturating_sub(indent.chars().count())
            .max(1);
        let chrome = columns * 3 + 1;
        let content = available.saturating_sub(chrome);
        let widths = if natural.iter().sum::<usize>() <= content {
            natural.clone()
        } else {
            match allocate_widths(&natural, &minima, content) {
                Some(widths) => widths,
                None => return self.write_stacked_table(out, indent, &header, &rows, available),
            }
        };

        let header_lines: Vec<Vec<Vec<Run>>> = header
            .iter()
            .zip(&widths)
            .map(|(cell, width)| wrap_runs(&cell.runs, *width))
            .collect();
        let row_lines: Vec<Vec<Vec<Vec<Run>>>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .zip(&widths)
                    .map(|(cell, width)| wrap_runs(&cell.runs, *width))
                    .collect()
            })
            .collect();

        let border = |left: &str, middle: &str, right: &str| {
            let segments: Vec<String> = widths.iter().map(|width| "─".repeat(width + 2)).collect();
            format!("{left}{}{right}", segments.join(middle))
        };

        out.write_all(indent.as_bytes())?;
        self.write_styled(out, CYAN, &border("┌", "┬", "┐"))?;
        writeln!(out)?;
        self.write_table_row(out, indent, &header_lines, &widths, &align)?;
        if !rows.is_empty() {
            out.write_all(indent.as_bytes())?;
            self.write_styled(out, CYAN, &border("├", "┼", "┤"))?;
            writeln!(out)?;
            for row in &row_lines {
                self.write_table_row(out, indent, row, &widths, &align)?;
            }
        }
        out.write_all(indent.as_bytes())?;
        self.write_styled(out, CYAN, &border("└", "┴", "┘"))?;
        writeln!(out)
    }

    fn write_table_row(
        &self,
        out: &mut impl Write,
        indent: &str,
        cells: &[Vec<Vec<Run>>],
        widths: &[usize],
        align: &[Align],
    ) -> io::Result<()> {
        let height = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
        for row_index in 0..height {
            out.write_all(indent.as_bytes())?;
            self.write_styled(out, CYAN, "│")?;
            for (index, width) in widths.iter().enumerate() {
                let line = cells
                    .get(index)
                    .and_then(|lines| lines.get(row_index))
                    .map_or(&[][..], Vec::as_slice);
                let used = runs_width(line).min(*width);
                let padding = width.saturating_sub(used);
                let (left, right) = match align[index] {
                    Align::Left => (0, padding),
                    Align::Right => (padding, 0),
                    Align::Center => (padding / 2, padding - padding / 2),
                };
                out.write_all(b" ")?;
                for _ in 0..left {
                    out.write_all(b" ")?;
                }
                self.write_runs(out, line)?;
                for _ in 0..right {
                    out.write_all(b" ")?;
                }
                self.write_styled(out, CYAN, " │")?;
            }
            writeln!(out)?;
        }
        Ok(())
    }

    fn write_stacked_table(
        &self,
        out: &mut impl Write,
        indent: &str,
        header: &[Cell],
        rows: &[Vec<Cell>],
        width: usize,
    ) -> io::Result<()> {
        if rows.is_empty() {
            let mut runs = Vec::new();
            for (index, cell) in header.iter().enumerate() {
                if index > 0 {
                    push_fragment(&mut runs, " · ", &[]);
                }
                append_runs(&mut runs, &cell.runs);
            }
            return self.write_stacked_lines(out, indent, &runs, width);
        }

        for (row_index, row) in rows.iter().enumerate() {
            if row_index > 0 {
                writeln!(out)?;
            }
            for (index, cell) in row.iter().enumerate() {
                if cell.runs.is_empty() {
                    continue;
                }
                let mut runs = Vec::new();
                if index > 0
                    && let Some(label) = header.get(index).filter(|label| !label.runs.is_empty())
                {
                    append_runs(&mut runs, &label.runs);
                    push_fragment(&mut runs, ": ", &[]);
                }
                append_runs(&mut runs, &cell.runs);
                self.write_stacked_lines(out, indent, &runs, width)?;
            }
        }
        Ok(())
    }

    fn write_stacked_lines(
        &self,
        out: &mut impl Write,
        indent: &str,
        runs: &[Run],
        width: usize,
    ) -> io::Result<()> {
        for line in wrap_runs(runs, width.saturating_sub(2).max(1)) {
            out.write_all(indent.as_bytes())?;
            out.write_all(b"  ")?;
            self.write_runs(out, &line)?;
            writeln!(out)?;
        }
        Ok(())
    }

    fn write_runs(&self, out: &mut impl Write, runs: &[Run]) -> io::Result<()> {
        if !self.color {
            for run in runs {
                out.write_all(run.text.as_bytes())?;
            }
            return Ok(());
        }

        for run in runs {
            out.write_all(RESET.as_bytes())?;
            for style in &run.styles {
                out.write_all(style.as_bytes())?;
            }
            out.write_all(run.text.as_bytes())?;
        }
        if !runs.is_empty() {
            out.write_all(RESET.as_bytes())?;
        }
        Ok(())
    }

    fn terminal_width(&self) -> usize {
        if self.width > 0 {
            self.width
        } else {
            terminal_width()
        }
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

fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(width, _)| width as usize)
        .unwrap_or(80)
        .max(1)
}

fn allocate_widths(natural: &[usize], minima: &[usize], available: usize) -> Option<Vec<usize>> {
    if natural.iter().sum::<usize>() <= available {
        return Some(natural.to_vec());
    }

    let minimum: usize = minima.iter().sum();
    if minimum > available {
        return None;
    }

    let flex: Vec<usize> = natural
        .iter()
        .zip(minima)
        .map(|(natural, minimum)| natural.saturating_sub(*minimum))
        .collect();
    let total_flex: usize = flex.iter().sum();
    let extra = available - minimum;
    let mut widths = minima.to_vec();

    let divisor = total_flex.max(1);
    let mut used = 0;
    for (index, flex) in flex.iter().enumerate() {
        let add = extra * flex / divisor;
        widths[index] += add;
        used += add;
    }

    let mut leftover = extra - used;
    while leftover > 0 {
        let mut progressed = false;
        for index in 0..widths.len() {
            if leftover == 0 {
                break;
            }
            if widths[index] < natural[index] {
                widths[index] += 1;
                leftover -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    Some(widths)
}

#[derive(Clone)]
struct Run {
    text: String,
    styles: Vec<&'static str>,
}

fn inline_runs(text: &str, base: &[&'static str]) -> Vec<Run> {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;

    let mut inline = Inline::new(base);
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
    styles: Vec<&'static str>,
    runs: Vec<Run>,
}

impl Inline {
    fn new(base: &[&'static str]) -> Self {
        Self {
            styles: base.to_vec(),
            runs: Vec::new(),
        }
    }

    fn text(&mut self, text: &str) {
        self.push_fragment(text, &[]);
    }

    fn push(&mut self, style: &'static str) {
        self.styles.push(style);
    }

    fn pop(&mut self) {
        self.styles.pop();
    }

    fn leaf(&mut self, style: &'static str, text: &str) {
        self.push_fragment(text, &[style]);
    }

    fn push_fragment(&mut self, text: &str, extra: &[&'static str]) {
        if text.is_empty() {
            return;
        }
        let mut styles = self.styles.clone();
        styles.extend_from_slice(extra);
        push_fragment(&mut self.runs, text, &styles);
    }

    fn finish(self) -> Vec<Run> {
        self.runs
    }
}

fn push_fragment(target: &mut Vec<Run>, text: &str, styles: &[&'static str]) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = target.last_mut()
        && last.styles.as_slice() == styles
    {
        last.text.push_str(text);
        return;
    }
    target.push(Run {
        text: text.to_string(),
        styles: styles.to_vec(),
    });
}

fn append_runs(target: &mut Vec<Run>, runs: &[Run]) {
    for run in runs {
        push_fragment(target, &run.text, &run.styles);
    }
}

fn words(runs: &[Run]) -> Vec<Vec<Run>> {
    let mut words = Vec::new();
    let mut current = Vec::new();
    for run in runs {
        for character in run.text.chars() {
            if character.is_whitespace() {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            } else {
                push_char(&mut current, character, &run.styles);
            }
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn push_char(target: &mut Vec<Run>, character: char, styles: &[&'static str]) {
    if let Some(last) = target.last_mut()
        && last.styles.as_slice() == styles
    {
        last.text.push(character);
        return;
    }
    target.push(Run {
        text: character.to_string(),
        styles: styles.to_vec(),
    });
}

fn wrap_runs(runs: &[Run], width: usize) -> Vec<Vec<Run>> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = Vec::new();
    let mut line_width = 0;

    for word in words(runs) {
        let word_width = runs_width(&word);
        if line.is_empty() {
            if word_width <= width {
                line = word;
                line_width = word_width;
            } else {
                let mut chunks = split_word(&word, width);
                if let Some(last) = chunks.pop() {
                    lines.extend(chunks);
                    line_width = runs_width(&last);
                    line = last;
                }
            }
        } else if line_width + 1 + word_width <= width {
            push_fragment(&mut line, " ", &[]);
            line_width += 1;
            append_runs(&mut line, &word);
            line_width += word_width;
        } else {
            lines.push(std::mem::take(&mut line));
            line_width = 0;
            if word_width <= width {
                line = word;
                line_width = word_width;
            } else {
                let mut chunks = split_word(&word, width);
                if let Some(last) = chunks.pop() {
                    lines.extend(chunks);
                    line_width = runs_width(&last);
                    line = last;
                }
            }
        }
    }

    if !line.is_empty() {
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

fn split_word(word: &[Run], width: usize) -> Vec<Vec<Run>> {
    let mut chunks = Vec::new();
    let mut chunk = Vec::new();
    let mut chunk_width = 0;

    for run in word {
        for character in run.text.chars() {
            let glyph_width = char_width(character);
            if chunk_width + glyph_width > width && !chunk.is_empty() {
                chunks.push(std::mem::take(&mut chunk));
                chunk_width = 0;
            }
            push_char(&mut chunk, character, &run.styles);
            chunk_width += glyph_width;
        }
    }

    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

fn runs_width(runs: &[Run]) -> usize {
    runs.iter().map(|run| text_width(&run.text)).sum()
}

fn text_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

fn char_width(character: char) -> usize {
    UnicodeWidthChar::width(character).unwrap_or(0)
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

    fn render_at(input: &str, width: usize) -> String {
        let mut renderer = Renderer::with_width(true, false, width);
        let mut out = Vec::new();
        renderer.write(input, &mut out).unwrap();
        renderer.finish(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn render_colored_at(input: &str, width: usize) -> String {
        let mut renderer = Renderer::with_width(true, true, width);
        let mut out = Vec::new();
        renderer.write(input, &mut out).unwrap();
        renderer.finish(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn assert_fits(output: &str, width: usize) {
        for line in output.lines() {
            assert!(
                text_width(line) <= width,
                "line is {} columns wide (max {width}): {line}",
                text_width(line)
            );
        }
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

    #[test]
    fn wraps_tables_to_the_terminal_width() {
        let input = "\
| Property | How it helps the huh API client |
| --- | --- |
| Memory safety without GC | No dangling pointers or buffer overruns can corrupt the request/response handling; the compiler guarantees all data lives long enough for the async-like streaming logic. |
| Strong typing | Enums and structs enforce only valid states, eliminating many classes of bugs. |
";
        let output = render_at(input, 60);
        assert_fits(&output, 60);
        assert!(output.contains("Memory"));
        assert!(output.contains("request/response"));
        assert!(output.contains("bugs."));
    }

    #[test]
    fn keeps_tables_at_their_natural_width_when_they_fit() {
        let input = "| Name | Link |\n|:---|---:|\n| Example | [docs](https://example.com) |\n";
        let output = render_at(input, 100);
        assert_fits(&output, 100);
        assert!(output.contains("┌"));
        assert!(output.contains("Example"));
    }

    #[test]
    fn stacks_tables_that_cannot_fit() {
        let input = "\
| LongHeader | OtherHeader |
| --- | --- |
| abcdefghijklmnop | qrstuvwxyzabcdef |
";
        let output = render_at(input, 20);
        assert_fits(&output, 20);
        assert!(output.contains("abcdefghijklmnop"));
        assert!(output.contains("OtherHeader:"));
        assert!(!output.contains('┌'));
    }

    #[test]
    fn aligns_wide_characters() {
        let output = render_at("| 名前 | Value |\n| --- | --- |\n| 日本 | ok |\n", 40);
        let widths: Vec<usize> = output
            .lines()
            .filter(|line| !line.is_empty())
            .map(text_width)
            .collect();
        assert!(widths.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn keeps_inline_styles_inside_wrapped_cells() {
        let input = "\
| A | B |
| --- | --- |
| **bold words here** | `code` and [link](https://example.com) text |
";
        let output = render_colored_at(input, 40);
        assert!(output.contains(BOLD));
        assert!(output.contains(YELLOW));
        assert!(output.contains(BLUE));
        assert!(!output.contains("**"));
    }
}
