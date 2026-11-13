// ============================================================================
// document.rs — Document Buffer, Multi-Cursor, Code Folding, Formatting & Undo
// ============================================================================

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: usize,
    pub col: usize, // char index within the line
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndentStyle {
    Spaces(usize),
    Tabs,
}

impl IndentStyle {
    pub fn unit_str(self) -> String {
        match self {
            Self::Spaces(n) => " ".repeat(n),
            Self::Tabs => "\t".to_string(),
        }
    }

    pub fn step_cols(self) -> usize {
        match self {
            Self::Spaces(n) => n.max(2),
            Self::Tabs => 4,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Spaces(n) => format!("Spaces: {}", n),
            Self::Tabs => "Tab Size: 4".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    InsertChar,
    DeleteChar,
    BlockOp,
}

#[derive(Clone, Debug)]
struct UndoSnapshot {
    lines: Vec<String>,
    cursor: Pos,
    extra_cursors: Vec<Pos>,
    anchor: Option<Pos>,
    kind: EditKind,
    timestamp: Instant,
}

pub struct Document {
    pub title: String,
    pub path: Option<PathBuf>,
    pub lines: Vec<String>,
    pub cursor: Pos,
    pub extra_cursors: Vec<Pos>,
    pub anchor: Option<Pos>,
    pub preferred_col: usize,
    pub scroll_y: usize,
    pub scroll_x: usize,
    pub dirty: bool,
    pub indent_style: IndentStyle,
    pub wrap_lines: bool,
    pub folded_headers: BTreeSet<usize>,
    pub bookmarks: BTreeSet<usize>,
    pub max_undo_history: usize,
    undo_stack: Vec<UndoSnapshot>,
    redo_stack: Vec<UndoSnapshot>,
}

impl Document {
    pub fn new_with_text(title: &str, path: Option<PathBuf>, text: &str) -> Self {
        let mut lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
        if text.ends_with('\n') || lines.is_empty() {
            lines.push(String::new());
        }
        Self {
            title: title.to_string(),
            path,
            lines,
            cursor: Pos::new(0, 0),
            extra_cursors: Vec::new(),
            anchor: None,
            preferred_col: 0,
            scroll_y: 0,
            scroll_x: 0,
            dirty: false,
            indent_style: IndentStyle::Spaces(4),
            wrap_lines: false,
            folded_headers: BTreeSet::new(),
            bookmarks: BTreeSet::new(),
            max_undo_history: 500,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn line_char_len(&self, line: usize) -> usize {
        self.lines
            .get(line)
            .map(|s| s.chars().count())
            .unwrap_or(0)
    }

    pub fn clamp_pos(&self, p: Pos) -> Pos {
        let line = p.line.min(self.lines.len().saturating_sub(1));
        let col = p.col.min(self.line_char_len(line));
        Pos::new(line, col)
    }

    pub fn char_to_byte_idx(s: &str, char_col: usize) -> usize {
        s.char_indices()
            .nth(char_col)
            .map(|(i, _)| i)
            .unwrap_or(s.len())
    }

    pub fn selection_range(&self) -> Option<(Pos, Pos)> {
        let anchor = self.anchor?;
        if anchor == self.cursor {
            None
        } else if anchor < self.cursor {
            Some((anchor, self.cursor))
        } else {
            Some((self.cursor, anchor))
        }
    }

    pub fn has_selection(&self) -> bool {
        self.selection_range().is_some()
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        if start.line == end.line {
            let line = &self.lines[start.line];
            let b0 = Self::char_to_byte_idx(line, start.col);
            let b1 = Self::char_to_byte_idx(line, end.col);
            return Some(line[b0..b1].to_string());
        }
        let mut out = String::new();
        let first = &self.lines[start.line];
        out.push_str(&first[Self::char_to_byte_idx(first, start.col)..]);
        out.push('\n');
        for l in (start.line + 1)..end.line {
            out.push_str(&self.lines[l]);
            out.push('\n');
        }
        let last = &self.lines[end.line];
        out.push_str(&last[..Self::char_to_byte_idx(last, end.col)]);
        Some(out)
    }

    // ── Multi-Cursor Management ──────────────────────────────────────────────

    pub fn clear_extra_cursors(&mut self) {
        self.extra_cursors.clear();
    }

    pub fn toggle_extra_cursor(&mut self, pos: Pos) {
        let p = self.clamp_pos(pos);
        if p == self.cursor {
            return;
        }
        if let Some(idx) = self.extra_cursors.iter().position(|&c| c == p) {
            self.extra_cursors.remove(idx);
        } else {
            self.extra_cursors.push(p);
            self.extra_cursors.sort();
            self.extra_cursors.dedup();
        }
    }

    pub fn add_cursor_vertical(&mut self, delta: isize) {
        let base = if delta < 0 {
            self.all_cursors().into_iter().min().unwrap_or(self.cursor)
        } else {
            self.all_cursors().into_iter().max().unwrap_or(self.cursor)
        };
        let target_line = (base.line as isize + delta).clamp(0, self.lines.len().saturating_sub(1) as isize) as usize;
        if target_line != base.line {
            let new_pos = self.clamp_pos(Pos::new(target_line, base.col));
            self.toggle_extra_cursor(new_pos);
        }
    }

    /// `Ctrl+D`: Selects word at cursor, or adds an extra cursor at the next match!
    pub fn add_cursor_next_occurrence(&mut self) {
        if !self.has_selection() {
            self.select_word_at(self.cursor);
            return;
        }
        let Some(needle) = self.selected_text() else {
            return;
        };
        if needle.is_empty() || needle.contains('\n') {
            return;
        }
        let needle_chars = needle.chars().count();
        let last_pos = self.all_cursors().into_iter().max().unwrap_or(self.cursor);

        // Search forward from `last_pos`
        for line_offset in 0..self.lines.len() {
            let l = (last_pos.line + line_offset) % self.lines.len();
            let line_str = &self.lines[l];
            let start_char = if line_offset == 0 { last_pos.col } else { 0 };
            let start_byte = Self::char_to_byte_idx(line_str, start_char);
            if let Some(rel_byte) = line_str[start_byte..].find(&needle) {
                let match_start_char = start_char + line_str[start_byte..(start_byte + rel_byte)].chars().count();
                let target_cursor = Pos::new(l, match_start_char + needle_chars);
                if target_cursor != self.cursor && !self.extra_cursors.contains(&target_cursor) {
                    self.extra_cursors.push(target_cursor);
                    self.extra_cursors.sort();
                    return;
                }
            }
        }
    }

    pub fn all_cursors(&self) -> Vec<Pos> {
        let mut v = Vec::with_capacity(1 + self.extra_cursors.len());
        v.push(self.cursor);
        for &c in &self.extra_cursors {
            if c != self.cursor {
                v.push(self.clamp_pos(c));
            }
        }
        v.sort();
        v.dedup();
        v
    }

    // ── Code Folding (Indentation-Based Blocks) ──────────────────────────────

    pub fn line_indent_cols(line: &str, tab_w: usize) -> Option<usize> {
        let mut cols = 0usize;
        for ch in line.chars() {
            match ch {
                ' ' => cols += 1,
                '\t' => cols += tab_w,
                _ => return Some(cols),
            }
        }
        None // Blank / whitespace-only line
    }

    /// Returns `Some(end_line)` if `line` is the header of a foldable indentation block.
    pub fn foldable_range(&self, line: usize) -> Option<usize> {
        if line + 1 >= self.lines.len() {
            return None;
        }
        let tab_w = self.indent_style.step_cols();
        let base_indent = Self::line_indent_cols(&self.lines[line], tab_w)?;

        let mut last_body_line = None;
        for l in (line + 1)..self.lines.len() {
            if let Some(ind) = Self::line_indent_cols(&self.lines[l], tab_w) {
                if ind > base_indent {
                    last_body_line = Some(l);
                } else {
                    break;
                }
            }
        }
        last_body_line.filter(|&end_l| end_l > line)
    }

    pub fn toggle_fold_at(&mut self, line: usize) -> bool {
        if self.foldable_range(line).is_some() {
            if !self.folded_headers.remove(&line) {
                self.folded_headers.insert(line);
            }
            true
        } else {
            false
        }
    }

    pub fn unfold_all(&mut self) {
        self.folded_headers.clear();
    }

    /// Computes the list of visible document line indices taking code folding into account.
    pub fn visible_doc_lines(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.lines.len());
        let mut l = 0usize;
        while l < self.lines.len() {
            out.push(l);
            if self.folded_headers.contains(&l) {
                if let Some(end_l) = self.foldable_range(l) {
                    l = end_l + 1;
                    continue;
                }
            }
            l += 1;
        }
        out
    }

    /// Sticky Scroll: finds up to 3 enclosing parent block headers above `top_line`.
    pub fn sticky_headers_for(&self, top_line: usize) -> Vec<usize> {
        if top_line == 0 || top_line >= self.lines.len() {
            return Vec::new();
        }
        let tab_w = self.indent_style.step_cols();
        let mut headers = Vec::new();
        for candidate in 0..top_line {
            if let Some(end_l) = self.foldable_range(candidate) {
                if end_l >= top_line {
                    headers.push((candidate, Self::line_indent_cols(&self.lines[candidate], tab_w).unwrap_or(0)));
                }
            }
        }
        headers.sort_by_key(|&(_, ind)| ind);
        headers.dedup_by_key(|&mut (_, ind)| ind);
        headers.into_iter().map(|(l, _)| l).take(3).collect()
    }

    // ── Matching Bracket / Quote Finder ──────────────────────────────────────

    pub fn find_matching_bracket(&self) -> Option<(Pos, Pos)> {
        let line_idx = self.cursor.line;
        if line_idx >= self.lines.len() {
            return None;
        }
        let chars: Vec<char> = self.lines[line_idx].chars().collect();
        let candidates = [self.cursor.col, self.cursor.col.saturating_sub(1)];

        for &col in &candidates {
            if col >= chars.len() {
                continue;
            }
            let ch = chars[col];
            let pos = Pos::new(line_idx, col);

            match ch {
                '(' | '[' | '{' => {
                    let close = match ch {
                        '(' => ')',
                        '[' => ']',
                        _ => '}',
                    };
                    let mut depth = 0i32;
                    for l in line_idx..self.lines.len() {
                        let l_chars: Vec<char> = self.lines[l].chars().collect();
                        let start_c = if l == line_idx { col } else { 0 };
                        for (c_idx, &c) in l_chars.iter().enumerate().skip(start_c) {
                            if c == ch {
                                depth += 1;
                            } else if c == close {
                                depth -= 1;
                                if depth == 0 {
                                    return Some((pos, Pos::new(l, c_idx)));
                                }
                            }
                        }
                    }
                }
                ')' | ']' | '}' => {
                    let open = match ch {
                        ')' => '(',
                        ']' => '[',
                        _ => '{',
                    };
                    let mut depth = 0i32;
                    for l in (0..=line_idx).rev() {
                        let l_chars: Vec<char> = self.lines[l].chars().collect();
                        let end_c = if l == line_idx { col } else { l_chars.len().saturating_sub(1) };
                        if l_chars.is_empty() {
                            continue;
                        }
                        for c_idx in (0..=end_c.min(l_chars.len() - 1)).rev() {
                            let c = l_chars[c_idx];
                            if c == ch {
                                depth += 1;
                            } else if c == open {
                                depth -= 1;
                                if depth == 0 {
                                    return Some((Pos::new(l, c_idx), pos));
                                }
                            }
                        }
                    }
                }
                '"' | '\'' | '`' => {
                    // Find paired quote on the same line
                    for (c_idx, &c) in chars.iter().enumerate() {
                        if c_idx != col && c == ch {
                            return Some((
                                Pos::new(line_idx, col.min(c_idx)),
                                Pos::new(line_idx, col.max(c_idx)),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    // ── Smart Undo / Redo ────────────────────────────────────────────────────

    pub fn push_undo(&mut self, kind: EditKind) {
        let now = Instant::now();
        self.redo_stack.clear();

        if kind != EditKind::BlockOp {
            if let Some(last) = self.undo_stack.last_mut() {
                if last.kind == kind && now.duration_since(last.timestamp) < Duration::from_millis(650) {
                    last.timestamp = now;
                    return;
                }
            }
        }

        if self.undo_stack.len() >= self.max_undo_history {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(UndoSnapshot {
            lines: self.lines.clone(),
            cursor: self.cursor,
            extra_cursors: self.extra_cursors.clone(),
            anchor: self.anchor,
            kind,
            timestamp: now,
        });
    }

    pub fn undo(&mut self) -> bool {
        if let Some(snap) = self.undo_stack.pop() {
            self.redo_stack.push(UndoSnapshot {
                lines: self.lines.clone(),
                cursor: self.cursor,
                extra_cursors: self.extra_cursors.clone(),
                anchor: self.anchor,
                kind: EditKind::BlockOp,
                timestamp: Instant::now(),
            });
            self.lines = snap.lines;
            self.cursor = self.clamp_pos(snap.cursor);
            self.extra_cursors = snap.extra_cursors.into_iter().map(|c| self.clamp_pos(c)).collect();
            self.anchor = snap.anchor.map(|a| self.clamp_pos(a));
            self.preferred_col = self.cursor.col;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(snap) = self.redo_stack.pop() {
            self.undo_stack.push(UndoSnapshot {
                lines: self.lines.clone(),
                cursor: self.cursor,
                extra_cursors: self.extra_cursors.clone(),
                anchor: self.anchor,
                kind: EditKind::BlockOp,
                timestamp: Instant::now(),
            });
            self.lines = snap.lines;
            self.cursor = self.clamp_pos(snap.cursor);
            self.extra_cursors = snap.extra_cursors.into_iter().map(|c| self.clamp_pos(c)).collect();
            self.anchor = snap.anchor.map(|a| self.clamp_pos(a));
            self.preferred_col = self.cursor.col;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn delete_selection_internal(&mut self) -> bool {
        let Some((start, end)) = self.selection_range() else {
            return false;
        };
        if start.line == end.line {
            let line = &mut self.lines[start.line];
            let b0 = Self::char_to_byte_idx(line, start.col);
            let b1 = Self::char_to_byte_idx(line, end.col);
            line.replace_range(b0..b1, "");
        } else {
            let tail = {
                let last = &self.lines[end.line];
                let b1 = Self::char_to_byte_idx(last, end.col);
                last[b1..].to_string()
            };
            let first = &mut self.lines[start.line];
            let b0 = Self::char_to_byte_idx(first, start.col);
            first.truncate(b0);
            first.push_str(&tail);
            self.lines.drain((start.line + 1)..=end.line);
        }
        self.cursor = start;
        self.anchor = None;
        self.preferred_col = self.cursor.col;
        self.dirty = true;
        true
    }

    // ── Editing Operations (Single & Multi-Cursor) ───────────────────────────

    pub fn insert_char(&mut self, ch: char) {
        // Multi-cursor simultaneous typing
        if !self.extra_cursors.is_empty() {
            self.push_undo(EditKind::InsertChar);
            self.anchor = None;
            let mut cursors = self.all_cursors();
            // Apply from bottom-right to top-left so line offsets don't shift earlier cursors
            for i in (0..cursors.len()).rev() {
                let p = cursors[i];
                let line = &mut self.lines[p.line];
                let b_idx = Self::char_to_byte_idx(line, p.col);
                line.insert(b_idx, ch);
                cursors[i].col += 1;
                for subsequent in cursors.iter_mut().skip(i + 1) {
                    if subsequent.line == p.line {
                        subsequent.col += 1;
                    }
                }
            }
            self.cursor = cursors[0];
            self.extra_cursors = cursors.into_iter().skip(1).collect();
            self.preferred_col = self.cursor.col;
            self.dirty = true;
            return;
        }

        let pair_close = match ch {
            '(' => Some(')'),
            '[' => Some(']'),
            '{' => Some('}'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '`' => Some('`'),
            _ => None,
        };

        // 1. Surround selection if text is selected and user types an opening bracket/quote
        if let (Some(close), Some((start, end))) = (pair_close, self.selection_range()) {
            self.push_undo(EditKind::BlockOp);
            if start.line == end.line {
                let line = &mut self.lines[start.line];
                let b1 = Self::char_to_byte_idx(line, end.col);
                line.insert(b1, close);
                let b0 = Self::char_to_byte_idx(line, start.col);
                line.insert(b0, ch);
                self.anchor = Some(Pos::new(start.line, start.col + 1));
                self.cursor = Pos::new(end.line, end.col + 1);
            } else {
                let last = &mut self.lines[end.line];
                let b1 = Self::char_to_byte_idx(last, end.col);
                last.insert(b1, close);
                let first = &mut self.lines[start.line];
                let b0 = Self::char_to_byte_idx(first, start.col);
                first.insert(b0, ch);
                self.anchor = Some(Pos::new(start.line, start.col + 1));
                self.cursor = Pos::new(end.line, end.col);
            }
            self.preferred_col = self.cursor.col;
            self.dirty = true;
            return;
        }

        // 2. Skip-over closing bracket/quote if already right after the cursor
        if matches!(ch, ')' | ']' | '}' | '"' | '\'' | '`') && !self.has_selection() {
            let line = &self.lines[self.cursor.line];
            if line.chars().nth(self.cursor.col) == Some(ch) {
                self.cursor.col += 1;
                self.preferred_col = self.cursor.col;
                return;
            }
        }

        self.push_undo(EditKind::InsertChar);
        if self.has_selection() {
            self.delete_selection_internal();
        }

        let line = &mut self.lines[self.cursor.line];
        let b_idx = Self::char_to_byte_idx(line, self.cursor.col);
        if let Some(close) = pair_close {
            let mut s = String::with_capacity(2);
            s.push(ch);
            s.push(close);
            line.insert_str(b_idx, &s);
            self.cursor.col += 1;
        } else {
            line.insert(b_idx, ch);
            self.cursor.col += 1;
        }
        self.preferred_col = self.cursor.col;
        self.dirty = true;
    }

    pub fn insert_newline_auto_indent(&mut self) {
        self.extra_cursors.clear();
        self.push_undo(EditKind::BlockOp);
        if self.has_selection() {
            self.delete_selection_internal();
        }

        let cur_line = self.lines[self.cursor.line].clone();
        let b_idx = Self::char_to_byte_idx(&cur_line, self.cursor.col);
        let before = &cur_line[..b_idx];
        let after = &cur_line[b_idx..];

        let leading_ws: String = before
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();

        let trimmed_before = before.trim_end();
        let opens_block = trimmed_before.ends_with('{')
            || trimmed_before.ends_with('(')
            || trimmed_before.ends_with('[')
            || trimmed_before.ends_with(':');

        let trimmed_after = after.trim_start();
        let closes_immediately = (trimmed_before.ends_with('{') && trimmed_after.starts_with('}'))
            || (trimmed_before.ends_with('(') && trimmed_after.starts_with(')'))
            || (trimmed_before.ends_with('[') && trimmed_after.starts_with(']'));

        let unit = self.indent_style.unit_str();
        let next_indent = if opens_block {
            format!("{}{}", leading_ws, unit)
        } else {
            leading_ws.clone()
        };

        self.lines[self.cursor.line] = before.to_string();

        if closes_immediately {
            let middle_line = next_indent.clone();
            let closing_line = format!("{}{}", leading_ws, after);
            self.lines.insert(self.cursor.line + 1, middle_line);
            self.lines.insert(self.cursor.line + 2, closing_line);
            self.cursor = Pos::new(self.cursor.line + 1, next_indent.chars().count());
        } else {
            let new_line = format!("{}{}", next_indent, after);
            self.lines.insert(self.cursor.line + 1, new_line);
            self.cursor = Pos::new(self.cursor.line + 1, next_indent.chars().count());
        }

        self.anchor = None;
        self.preferred_col = self.cursor.col;
        self.dirty = true;
    }

    pub fn insert_tab(&mut self) {
        self.push_undo(EditKind::InsertChar);
        if self.has_selection() {
            self.delete_selection_internal();
        }
        let unit = self.indent_style.unit_str();
        let line = &mut self.lines[self.cursor.line];
        let b_idx = Self::char_to_byte_idx(line, self.cursor.col);
        line.insert_str(b_idx, &unit);
        self.cursor.col += unit.chars().count();
        self.preferred_col = self.cursor.col;
        self.dirty = true;
    }

    pub fn insert_str(&mut self, text: &str) {
        self.push_undo(EditKind::BlockOp);
        if self.has_selection() {
            self.delete_selection_internal();
        }
        let normalized = text.replace("\r\n", "\n");
        let parts: Vec<&str> = normalized.split('\n').collect();
        if parts.len() == 1 {
            let line = &mut self.lines[self.cursor.line];
            let b_idx = Self::char_to_byte_idx(line, self.cursor.col);
            line.insert_str(b_idx, parts[0]);
            self.cursor.col += parts[0].chars().count();
        } else {
            let cur_line = self.lines[self.cursor.line].clone();
            let b_idx = Self::char_to_byte_idx(&cur_line, self.cursor.col);
            let before = &cur_line[..b_idx];
            let after = &cur_line[b_idx..];

            self.lines[self.cursor.line] = format!("{}{}", before, parts[0]);
            for (i, part) in parts.iter().enumerate().skip(1).take(parts.len() - 2) {
                self.lines.insert(self.cursor.line + i, (*part).to_string());
            }
            let last_part = parts[parts.len() - 1];
            let last_line = format!("{}{}", last_part, after);
            let target_line = self.cursor.line + parts.len() - 1;
            self.lines.insert(target_line, last_line);
            self.cursor = Pos::new(target_line, last_part.chars().count());
        }
        self.anchor = None;
        self.preferred_col = self.cursor.col;
        self.dirty = true;
    }

    pub fn backspace(&mut self) {
        if !self.extra_cursors.is_empty() {
            self.push_undo(EditKind::DeleteChar);
            let mut cursors = self.all_cursors();
            for i in (0..cursors.len()).rev() {
                let p = cursors[i];
                if p.col > 0 {
                    let line = &mut self.lines[p.line];
                    let b0 = Self::char_to_byte_idx(line, p.col - 1);
                    let b1 = Self::char_to_byte_idx(line, p.col);
                    line.replace_range(b0..b1, "");
                    cursors[i].col -= 1;
                    for subsequent in cursors.iter_mut().skip(i + 1) {
                        if subsequent.line == p.line {
                            subsequent.col = subsequent.col.saturating_sub(1);
                        }
                    }
                }
            }
            self.cursor = cursors[0];
            self.extra_cursors = cursors.into_iter().skip(1).collect();
            self.preferred_col = self.cursor.col;
            self.dirty = true;
            return;
        }

        self.push_undo(EditKind::DeleteChar);
        if self.delete_selection_internal() {
            return;
        }
        if self.cursor.col > 0 {
            let line = &mut self.lines[self.cursor.line];
            let prev_char = line.chars().nth(self.cursor.col - 1);
            let next_char = line.chars().nth(self.cursor.col);
            let is_empty_pair = matches!(
                (prev_char, next_char),
                (Some('('), Some(')'))
                    | (Some('['), Some(']'))
                    | (Some('{'), Some('}'))
                    | (Some('"'), Some('"'))
                    | (Some('\''), Some('\''))
                    | (Some('`'), Some('`'))
            );
            if is_empty_pair {
                let b0 = Self::char_to_byte_idx(line, self.cursor.col - 1);
                let b1 = Self::char_to_byte_idx(line, self.cursor.col + 1);
                line.replace_range(b0..b1, "");
                self.cursor.col -= 1;
            } else {
                let b0 = Self::char_to_byte_idx(line, self.cursor.col - 1);
                let b1 = Self::char_to_byte_idx(line, self.cursor.col);
                line.replace_range(b0..b1, "");
                self.cursor.col -= 1;
            }
            self.preferred_col = self.cursor.col;
            self.dirty = true;
        } else if self.cursor.line > 0 {
            let tail = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            let prev_len = self.line_char_len(self.cursor.line);
            self.lines[self.cursor.line].push_str(&tail);
            self.cursor.col = prev_len;
            self.preferred_col = self.cursor.col;
            self.dirty = true;
        }
    }

    pub fn delete_forward(&mut self) {
        self.push_undo(EditKind::DeleteChar);
        if self.delete_selection_internal() {
            return;
        }
        let len = self.line_char_len(self.cursor.line);
        if self.cursor.col < len {
            let line = &mut self.lines[self.cursor.line];
            let b0 = Self::char_to_byte_idx(line, self.cursor.col);
            let b1 = Self::char_to_byte_idx(line, self.cursor.col + 1);
            line.replace_range(b0..b1, "");
            self.dirty = true;
        } else if self.cursor.line + 1 < self.lines.len() {
            let next = self.lines.remove(self.cursor.line + 1);
            self.lines[self.cursor.line].push_str(&next);
            self.dirty = true;
        }
    }

    // ── Line Operations: Move, Duplicate, Toggle Comment, Format Document ────

    pub fn selected_line_span(&self) -> (usize, usize) {
        if let Some((s, e)) = self.selection_range() {
            let end_l = if e.col == 0 && e.line > s.line {
                e.line - 1
            } else {
                e.line
            };
            (s.line, end_l)
        } else {
            (self.cursor.line, self.cursor.line)
        }
    }

    pub fn move_lines_up(&mut self) {
        let (l0, l1) = self.selected_line_span();
        if l0 == 0 {
            return;
        }
        self.push_undo(EditKind::BlockOp);
        let above = self.lines.remove(l0 - 1);
        self.lines.insert(l1, above);
        self.cursor.line -= 1;
        if let Some(a) = &mut self.anchor {
            a.line = a.line.saturating_sub(1);
        }
        self.dirty = true;
    }

    pub fn move_lines_down(&mut self) {
        let (l0, l1) = self.selected_line_span();
        if l1 + 1 >= self.lines.len() {
            return;
        }
        self.push_undo(EditKind::BlockOp);
        let below = self.lines.remove(l1 + 1);
        self.lines.insert(l0, below);
        self.cursor.line += 1;
        if let Some(a) = &mut self.anchor {
            a.line += 1;
        }
        self.dirty = true;
    }

    pub fn duplicate_lines(&mut self) {
        let (l0, l1) = self.selected_line_span();
        self.push_undo(EditKind::BlockOp);
        let block: Vec<String> = self.lines[l0..=l1].to_vec();
        let count = block.len();
        for (i, line) in block.into_iter().enumerate() {
            self.lines.insert(l1 + 1 + i, line);
        }
        self.cursor.line += count;
        if let Some(a) = &mut self.anchor {
            a.line += count;
        }
        self.dirty = true;
    }

    pub fn toggle_comment(&mut self, prefix: &str) {
        let (l0, l1) = self.selected_line_span();
        self.push_undo(EditKind::BlockOp);

        let all_commented = (l0..=l1)
            .filter(|&l| !self.lines[l].trim().is_empty())
            .all(|l| self.lines[l].trim_start().starts_with(prefix));

        let prefix_with_space = format!("{} ", prefix);
        for l in l0..=l1 {
            let line = &mut self.lines[l];
            if line.trim().is_empty() {
                continue;
            }
            if all_commented {
                if let Some(pos) = line.find(&prefix_with_space) {
                    line.replace_range(pos..(pos + prefix_with_space.len()), "");
                } else if let Some(pos) = line.find(prefix) {
                    line.replace_range(pos..(pos + prefix.len()), "");
                }
            } else {
                let first_non_ws = line.find(|c: char| c != ' ' && c != '\t').unwrap_or(0);
                line.insert_str(first_non_ws, &prefix_with_space);
            }
        }
        self.cursor = self.clamp_pos(self.cursor);
        self.dirty = true;
    }

    /// Formats the entire document (uses `rustfmt` for Rust if installed, else built-in structural formatter).
    pub fn format_document(&mut self) {
        self.push_undo(EditKind::BlockOp);

        let is_rust = self.title.ends_with(".rs")
            || self
                .path
                .as_ref()
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                == Some("rs");

        if is_rust {
            if let Ok(mut child) = Command::new("rustfmt")
                .arg("--emit")
                .arg("stdout")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(self.text().as_bytes());
                }
                if let Ok(output) = child.wait_with_output() {
                    if output.status.success() {
                        if let Ok(formatted) = String::from_utf8(output.stdout) {
                            if !formatted.trim().is_empty() {
                                let mut new_lines: Vec<String> =
                                    formatted.lines().map(|s| s.to_string()).collect();
                                if formatted.ends_with('\n') {
                                    new_lines.push(String::new());
                                }
                                self.lines = new_lines;
                                self.cursor = self.clamp_pos(self.cursor);
                                self.dirty = true;
                                return;
                            }
                        }
                    }
                }
            }
        }

        // Built-in multi-language structural indentation & whitespace formatter
        let unit = self.indent_style.unit_str();
        let mut depth = 0usize;
        let mut formatted_lines = Vec::with_capacity(self.lines.len());

        for raw_line in &self.lines {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                formatted_lines.push(String::new());
                continue;
            }

            let leading_closes = trimmed
                .chars()
                .take_while(|&c| matches!(c, '}' | ')' | ']'))
                .count();
            let line_depth = depth.saturating_sub(leading_closes);
            let indent_prefix = unit.repeat(line_depth);
            formatted_lines.push(format!("{}{}", indent_prefix, trimmed));

            // Update depth for subsequent lines (ignoring characters inside strings/comments)
            let mut in_str: Option<char> = None;
            let mut prev = '\0';
            let chars: Vec<char> = trimmed.chars().collect();
            for (idx, &c) in chars.iter().enumerate() {
                if let Some(q) = in_str {
                    if c == q && prev != '\\' {
                        in_str = None;
                    }
                } else {
                    if c == '/' && chars.get(idx + 1) == Some(&'/') {
                        break;
                    }
                    if c == '#' {
                        break;
                    }
                    if matches!(c, '"' | '\'') {
                        in_str = Some(c);
                    } else if matches!(c, '{' | '(' | '[') {
                        depth += 1;
                    } else if matches!(c, '}' | ')' | ']') {
                        depth = depth.saturating_sub(1);
                    }
                }
                prev = c;
            }
        }

        self.lines = formatted_lines;
        self.cursor = self.clamp_pos(self.cursor);
        self.dirty = true;
    }

    // ── Cursor Navigation ────────────────────────────────────────────────────

    pub fn move_left(&mut self, select: bool) {
        self.update_anchor_for_move(select);
        if !select && self.has_selection() {
            if let Some((start, _)) = self.selection_range() {
                self.cursor = start;
                self.anchor = None;
                self.preferred_col = self.cursor.col;
                return;
            }
        }
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.line_char_len(self.cursor.line);
        }
        self.preferred_col = self.cursor.col;
    }

    pub fn move_right(&mut self, select: bool) {
        self.update_anchor_for_move(select);
        if !select && self.has_selection() {
            if let Some((_, end)) = self.selection_range() {
                self.cursor = end;
                self.anchor = None;
                self.preferred_col = self.cursor.col;
                return;
            }
        }
        let len = self.line_char_len(self.cursor.line);
        if self.cursor.col < len {
            self.cursor.col += 1;
        } else if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
        self.preferred_col = self.cursor.col;
    }

    pub fn move_up(&mut self, select: bool) {
        self.update_anchor_for_move(select);
        let vis = self.visible_doc_lines();
        if let Some(pos) = vis.iter().position(|&l| l >= self.cursor.line) {
            if pos > 0 {
                self.cursor.line = vis[pos - 1];
                self.cursor.col = self.preferred_col.min(self.line_char_len(self.cursor.line));
            }
        }
    }

    pub fn move_down(&mut self, select: bool) {
        self.update_anchor_for_move(select);
        let vis = self.visible_doc_lines();
        if let Some(pos) = vis.iter().position(|&l| l >= self.cursor.line) {
            if pos + 1 < vis.len() {
                self.cursor.line = vis[pos + 1];
                self.cursor.col = self.preferred_col.min(self.line_char_len(self.cursor.line));
            }
        }
    }

    pub fn move_home(&mut self, select: bool) {
        self.update_anchor_for_move(select);
        let first_non_ws = self.lines[self.cursor.line]
            .chars()
            .position(|c| c != ' ' && c != '\t')
            .unwrap_or(0);
        self.cursor.col = if self.cursor.col == first_non_ws {
            0
        } else {
            first_non_ws
        };
        self.preferred_col = self.cursor.col;
    }

    pub fn move_end(&mut self, select: bool) {
        self.update_anchor_for_move(select);
        self.cursor.col = self.line_char_len(self.cursor.line);
        self.preferred_col = self.cursor.col;
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(Pos::new(0, 0));
        let last = self.lines.len().saturating_sub(1);
        self.cursor = Pos::new(last, self.line_char_len(last));
        self.preferred_col = self.cursor.col;
    }

    pub fn select_word_at(&mut self, pos: Pos) {
        let p = self.clamp_pos(pos);
        let chars: Vec<char> = self.lines[p.line].chars().collect();
        if chars.is_empty() {
            return;
        }
        let idx = p.col.min(chars.len().saturating_sub(1));
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        if !is_word(chars[idx]) {
            self.anchor = Some(Pos::new(p.line, idx));
            self.cursor = Pos::new(p.line, (idx + 1).min(chars.len()));
            return;
        }
        let mut start = idx;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        let mut end = idx + 1;
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
        self.anchor = Some(Pos::new(p.line, start));
        self.cursor = Pos::new(p.line, end);
        self.preferred_col = self.cursor.col;
    }

    fn update_anchor_for_move(&mut self, select: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else if !self.has_selection() {
            self.anchor = None;
        }
    }

    pub fn ensure_cursor_visible(&mut self, visible_rows: usize, visible_cols: usize) {
        let v_rows = visible_rows.max(1);
        if self.cursor.line < self.scroll_y {
            self.scroll_y = self.cursor.line;
        } else if self.cursor.line >= self.scroll_y + v_rows {
            self.scroll_y = self.cursor.line + 1 - v_rows;
        }

        if self.wrap_lines {
            self.scroll_x = 0;
        } else {
            let v_cols = visible_cols.max(8);
            if self.cursor.col < self.scroll_x {
                self.scroll_x = self.cursor.col.saturating_sub(4);
            } else if self.cursor.col + 4 >= self.scroll_x + v_cols {
                self.scroll_x = (self.cursor.col + 4).saturating_sub(v_cols);
            }
        }
    }

    // ── Search & Replace Engine ──────────────────────────────────────────────

    pub fn find_all_matches(&self, query: &str) -> Vec<(Pos, Pos)> {
        if query.is_empty() {
            return Vec::new();
        }
        let q_lower = query.to_lowercase();
        let q_chars = query.chars().count();
        let mut matches = Vec::new();

        for (l_idx, line) in self.lines.iter().enumerate() {
            let l_lower = line.to_lowercase();
            let mut search_byte = 0;
            while let Some(rel) = l_lower[search_byte..].find(&q_lower) {
                let b_start = search_byte + rel;
                let c_start = line[..b_start].chars().count();
                let c_end = c_start + q_chars;
                matches.push((Pos::new(l_idx, c_start), Pos::new(l_idx, c_end)));
                search_byte = b_start + q_lower.len().max(1);
            }
        }
        matches
    }

    pub fn find_next(&mut self, query: &str) -> Option<(usize, usize)> {
        let matches = self.find_all_matches(query);
        if matches.is_empty() {
            return None;
        }
        let idx = matches
            .iter()
            .position(|(s, _)| *s >= self.cursor)
            .unwrap_or(0);
        let (s, e) = matches[idx];
        self.anchor = Some(s);
        self.cursor = e;
        self.preferred_col = e.col;
        Some((idx + 1, matches.len()))
    }

    pub fn find_prev(&mut self, query: &str) -> Option<(usize, usize)> {
        let matches = self.find_all_matches(query);
        if matches.is_empty() {
            return None;
        }
        let cut = self.selection_range().map(|(s, _)| s).unwrap_or(self.cursor);
        let idx = matches
            .iter()
            .rposition(|(s, _)| *s < cut)
            .unwrap_or(matches.len() - 1);
        let (s, e) = matches[idx];
        self.anchor = Some(s);
        self.cursor = e;
        self.preferred_col = e.col;
        Some((idx + 1, matches.len()))
    }

    pub fn replace_current_or_next(&mut self, query: &str, replacement: &str) -> bool {
        if query.is_empty() {
            return false;
        }
        if let Some(sel) = self.selected_text() {
            if sel.eq_ignore_ascii_case(query) {
                self.insert_str(replacement);
                let _ = self.find_next(query);
                return true;
            }
        }
        if self.find_next(query).is_some() {
            self.insert_str(replacement);
            return true;
        }
        false
    }

    pub fn replace_all(&mut self, query: &str, replacement: &str) -> usize {
        let matches = self.find_all_matches(query);
        if matches.is_empty() {
            return 0;
        }
        self.push_undo(EditKind::BlockOp);
        for &(s, e) in matches.iter().rev() {
            let line = &mut self.lines[s.line];
            let b0 = Self::char_to_byte_idx(line, s.col);
            let b1 = Self::char_to_byte_idx(line, e.col);
            line.replace_range(b0..b1, replacement);
        }
        self.anchor = None;
        self.cursor = self.clamp_pos(self.cursor);
        self.dirty = true;
        matches.len()
    }

    // ── Vim Word & Line Helpers ──────────────────────────────────────────────

    pub fn move_word_forward(&mut self, select: bool) {
        self.update_anchor_for_move(select);
        let chars: Vec<char> = self.lines[self.cursor.line].chars().collect();
        let mut c = self.cursor.col;
        while c < chars.len() && !chars[c].is_whitespace() {
            c += 1;
        }
        while c < chars.len() && chars[c].is_whitespace() {
            c += 1;
        }
        if c < chars.len() {
            self.cursor.col = c;
        } else if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            self.cursor.col = self.lines[self.cursor.line]
                .chars()
                .position(|ch| !ch.is_whitespace())
                .unwrap_or(0);
        } else {
            self.cursor.col = chars.len();
        }
        self.preferred_col = self.cursor.col;
    }

    pub fn move_word_backward(&mut self, select: bool) {
        self.update_anchor_for_move(select);
        if self.cursor.col == 0 {
            if self.cursor.line > 0 {
                self.cursor.line -= 1;
                self.cursor.col = self.line_char_len(self.cursor.line);
            }
            self.preferred_col = self.cursor.col;
            return;
        }
        let chars: Vec<char> = self.lines[self.cursor.line].chars().collect();
        let mut c = self.cursor.col.min(chars.len());
        while c > 0 && chars[c - 1].is_whitespace() {
            c -= 1;
        }
        while c > 0 && !chars[c - 1].is_whitespace() {
            c -= 1;
        }
        self.cursor.col = c;
        self.preferred_col = self.cursor.col;
    }

    pub fn delete_current_line(&mut self) -> String {
        self.push_undo(EditKind::BlockOp);
        let removed = if self.lines.len() > 1 {
            let r = self.lines.remove(self.cursor.line);
            if self.cursor.line >= self.lines.len() {
                self.cursor.line = self.lines.len() - 1;
            }
            r
        } else {
            let r = self.lines[0].clone();
            self.lines[0].clear();
            r
        };
        self.cursor = self.clamp_pos(self.cursor);
        self.anchor = None;
        self.dirty = true;
        removed
    }

    // ── Bookmarks & Go to Line ───────────────────────────────────────────────

    pub fn toggle_bookmark(&mut self) {
        let l = self.cursor.line;
        if !self.bookmarks.remove(&l) {
            self.bookmarks.insert(l);
        }
    }

    pub fn next_bookmark(&mut self) -> bool {
        if self.bookmarks.is_empty() {
            return false;
        }
        let next = self
            .bookmarks
            .range((self.cursor.line + 1)..)
            .next()
            .or_else(|| self.bookmarks.iter().next())
            .copied();
        if let Some(target) = next {
            if target < self.lines.len() {
                self.cursor = Pos::new(target, 0);
                self.anchor = None;
                return true;
            }
        }
        false
    }

    pub fn go_to_line(&mut self, one_based: usize) {
        let target = one_based.saturating_sub(1).min(self.lines.len().saturating_sub(1));
        self.cursor = Pos::new(target, 0);
        self.anchor = None;
        self.preferred_col = 0;
    }

    // ── Smart Autocomplete Engine ────────────────────────────────────────────

    pub fn current_word_prefix(&self) -> Option<String> {
        let line = self.lines.get(self.cursor.line)?;
        let chars: Vec<char> = line.chars().collect();
        let end = self.cursor.col.min(chars.len());
        if end == 0 {
            return None;
        }
        let mut start = end;
        while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
            start -= 1;
        }
        if end - start < 2 {
            return None;
        }
        let prefix: String = chars[start..end].iter().collect();
        if prefix.chars().next()?.is_ascii_digit() {
            return None;
        }
        Some(prefix)
    }

    pub fn completion_candidates(&self) -> Vec<String> {
        let Some(prefix) = self.current_word_prefix() else {
            return Vec::new();
        };
        let p_lower = prefix.to_lowercase();
        let mut set = BTreeSet::new();

        const KEYWORDS: &[&str] = &[
            "fn", "pub", "struct", "impl", "enum", "trait", "match", "let", "mut", "const",
            "static", "return", "async", "await", "use", "mod", "crate", "super", "where",
            "Self", "String", "Vec", "Option", "Result", "Some", "None", "Ok", "Err", "Box",
            "Rc", "Arc", "RefCell", "Mutex", "HashMap", "BTreeMap", "BTreeSet", "println!",
            "format!", "Default", "Clone", "Copy", "Debug", "PartialEq", "Eq", "Serialize",
            "Deserialize", "function", "interface", "export", "import", "from", "class", "def",
        ];

        for &kw in KEYWORDS {
            if kw != prefix && kw.to_lowercase().starts_with(&p_lower) {
                set.insert(kw.to_string());
            }
        }

        for line in &self.lines {
            for word in line.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if word.len() >= 3 && word != prefix && word.to_lowercase().starts_with(&p_lower) {
                    set.insert(word.to_string());
                }
            }
        }

        let mut out: Vec<String> = set.into_iter().collect();
        out.sort_by_key(|w| (!w.starts_with(&prefix), w.len()));
        out.truncate(7);
        out
    }

    pub fn apply_completion(&mut self, chosen: &str) {
        let Some(prefix) = self.current_word_prefix() else {
            return;
        };
        let p_chars = prefix.chars().count();
        self.push_undo(EditKind::BlockOp);
        let line = &mut self.lines[self.cursor.line];
        let start_col = self.cursor.col.saturating_sub(p_chars);
        let b0 = Self::char_to_byte_idx(line, start_col);
        let b1 = Self::char_to_byte_idx(line, self.cursor.col);
        line.replace_range(b0..b1, chosen);
        self.cursor.col = start_col + chosen.chars().count();
        self.preferred_col = self.cursor.col;
        self.dirty = true;
    }
}
