// ============================================================================
// canvas.rs — High-Performance FLTK IDE Canvas (Sidebar, Minimap, Autocomplete)
// ============================================================================

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use fltk::{
    app,
    draw,
    enums::{Align, Color, Cursor, Event, Font, Key, Shortcut},
    frame::Frame,
    menu::MenuItem,
    prelude::*,
    widget::Widget,
    widget_extends,
};

use crate::config::AppConfig;
use crate::document::{Document, EditKind, IndentStyle, Pos};
use crate::syntax::{StyledSpan, SyntaxEngine};
use crate::themes::{EditorTheme, Rgb};

pub const SIDEBAR_WIDTH: i32 = 210;
pub const GUTTER_WIDTH: i32 = 72;
pub const MINIMAP_WIDTH: i32 = 74;
pub const SCROLLBAR_SIZE: i32 = 12;

#[derive(Clone, Debug)]
pub struct SidebarEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_parent: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VimMode {
    Off,
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl VimMode {
    pub fn badge(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::Normal => " [VIM: NORMAL] ",
            Self::Insert => " [VIM: INSERT] ",
            Self::Visual => " [VIM: VISUAL] ",
            Self::VisualLine => " [VIM: V-LINE] ",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode {
    None,
    Find { replace: bool, focus_replace: bool },
    GoToLine,
    CommandPalette,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteAction {
    FormatDocument,
    ToggleComment,
    DuplicateLine,
    MoveLineUp,
    MoveLineDown,
    AddCursorNextOccurrence,
    ToggleSidebar,
    ToggleMinimap,
    ToggleBookmark,
    NextBookmark,
    GoToLine,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleWordWrap,
    ToggleStickyScroll,
    ToggleColorPreviews,
    ToggleRainbowBrackets,
    ToggleWhitespace,
    ToggleIndentGuides,
    ToggleFoldAtCursor,
    UnfoldAll,
    ToggleVimMode,
    OpenFind,
    OpenReplace,
    NewTab,
    CloseFile,
    IndentSpaces4,
    IndentSpaces2,
    IndentTabs,
    AboutDeveloper,
    SetTheme(usize),
}

pub struct PaletteEntry {
    pub label: String,
    pub shortcut: &'static str,
    pub action: PaletteAction,
}

pub struct EditorState {
    pub docs: Vec<Document>,
    pub active_doc: usize,
    pub theme_idx: usize,
    pub font_size: i32,
    pub syntax: SyntaxEngine,
    pub show_sidebar: bool,
    pub show_minimap: bool,
    pub show_whitespace: bool,
    pub show_indent_guides: bool,
    pub rainbow_brackets: bool,
    pub sticky_scroll: bool,
    pub inline_color_previews: bool,
    pub vim_mode: VimMode,
    pub vim_pending_key: Option<char>,
    pub vim_yank_buffer: String,
    pub overlay: OverlayMode,
    pub find_query: String,
    pub replace_query: String,
    pub goto_query: String,
    pub palette_query: String,
    pub palette_selected: usize,
    pub autocomplete_open: bool,
    pub autocomplete_selected: usize,
    pub workspace_dir: PathBuf,
    pub sidebar_entries: Vec<SidebarEntry>,
    pub sidebar_scroll: usize,
    pub dragging_vscroll: Option<(i32, usize)>,
    pub dragging_minimap: bool,
    pub hover_vscroll: bool,
}

impl EditorState {
    pub fn new() -> Self {
        let cfg = AppConfig::load();
        let workspace_dir = cfg
            .workspace_dir
            .clone()
            .filter(|p| p.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        let mut docs = Vec::new();
        for path in &cfg.open_files {
            if path.is_file() {
                if let Ok(content) = fs::read_to_string(path) {
                    let title = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("file")
                        .to_string();
                    docs.push(Document::new_with_text(&title, Some(path.clone()), &content));
                }
            }
        }

        if docs.is_empty() {
            let rust_demo = r##"// Welcome to Studio Code Editor — High-Performance FLTK + Syntect IDE
use std::collections::HashMap;

pub struct AudioProcessor {
    pub sample_rate: u32,
    pub channels: usize,
    pub primary_color: &'static str, // Inline color preview: #7AA2F7
    pub emerald_glow: &'static str,  // Inline color preview: #10B981
    pub coral_accent: u32,           // Hex literal preview:  0xFB923C
}

impl AudioProcessor {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            channels: 2,
            primary_color: "#7AA2F7",
            emerald_glow: "#10B981",
            coral_accent: 0xFB923C,
        }
    }

    pub fn process_frame(&self, samples: &[f32]) -> Vec<f32> {
        let mut output = Vec::with_capacity(samples.len());
        for &s in samples {
            if s.abs() > 0.0001 {
                let clamped = s.clamp(-1.0, 1.0);
                output.push((clamped * 0.95));
            }
        }
        output
    }
}
"##;
            let second_demo = r##"# Project Configuration (TOML)
[package]
name = "studio_code_editor"
version = "1.0.0"
edition = "2024"

[theme]
primary = "#7AA2F7"
emerald = "#10B981"
violet  = "rgb(189, 147, 249)"
"##;
            let mut d1 = Document::new_with_text("main.rs", None, rust_demo);
            d1.bookmarks.insert(11);
            docs.push(d1);
            docs.push(Document::new_with_text("config.toml", None, second_demo));
        }

        let active_doc = cfg.active_doc.min(docs.len().saturating_sub(1));

        let mut st = Self {
            docs,
            active_doc,
            theme_idx: cfg.theme_idx % EditorTheme::all().len(),
            font_size: cfg.font_size.clamp(10, 32),
            syntax: SyntaxEngine::new(),
            show_sidebar: cfg.show_sidebar,
            show_minimap: cfg.show_minimap,
            show_whitespace: cfg.show_whitespace,
            show_indent_guides: cfg.show_indent_guides,
            rainbow_brackets: cfg.rainbow_brackets,
            sticky_scroll: cfg.sticky_scroll,
            inline_color_previews: cfg.inline_color_previews,
            vim_mode: if cfg.vim_enabled {
                VimMode::Normal
            } else {
                VimMode::Off
            },
            vim_pending_key: None,
            vim_yank_buffer: String::new(),
            overlay: OverlayMode::None,
            find_query: String::new(),
            replace_query: String::new(),
            goto_query: String::new(),
            palette_query: String::new(),
            palette_selected: 0,
            autocomplete_open: false,
            autocomplete_selected: 0,
            workspace_dir,
            sidebar_entries: Vec::new(),
            sidebar_scroll: 0,
            dragging_vscroll: None,
            dragging_minimap: false,
            hover_vscroll: false,
        };
        st.refresh_sidebar();
        st
    }

    pub fn line_height(&self) -> i32 {
        ((self.font_size as f32) * 1.48).round().max(16.0) as i32
    }

    pub fn sidebar_w(&self) -> i32 {
        if self.show_sidebar { SIDEBAR_WIDTH } else { 0 }
    }

    pub fn minimap_w(&self) -> i32 {
        if self.show_minimap { MINIMAP_WIDTH } else { 0 }
    }

    pub fn save_session(&self) {
        let open_files: Vec<PathBuf> = self
            .docs
            .iter()
            .filter_map(|d| d.path.clone())
            .collect();
        let cfg = AppConfig {
            theme_idx: self.theme_idx,
            font_size: self.font_size,
            show_minimap: self.show_minimap,
            show_sidebar: self.show_sidebar,
            show_whitespace: self.show_whitespace,
            show_indent_guides: self.show_indent_guides,
            rainbow_brackets: self.rainbow_brackets,
            sticky_scroll: self.sticky_scroll,
            inline_color_previews: self.inline_color_previews,
            vim_enabled: self.vim_mode != VimMode::Off,
            workspace_dir: Some(self.workspace_dir.clone()),
            open_files,
            active_doc: self.active_doc,
        };
        cfg.save();
    }

    pub fn refresh_sidebar(&mut self) {
        self.sidebar_entries.clear();
        if let Some(parent) = self.workspace_dir.parent() {
            self.sidebar_entries.push(SidebarEntry {
                name: ".. (Parent Directory)".to_string(),
                path: parent.to_path_buf(),
                is_dir: true,
                is_parent: true,
            });
        }
        if let Ok(rd) = fs::read_dir(&self.workspace_dir) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();
            for entry in rd.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                if path.is_dir() {
                    dirs.push(SidebarEntry {
                        name: name.to_string(),
                        path,
                        is_dir: true,
                        is_parent: false,
                    });
                } else {
                    files.push(SidebarEntry {
                        name: name.to_string(),
                        path,
                        is_dir: false,
                        is_parent: false,
                    });
                }
            }
            dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            self.sidebar_entries.extend(dirs);
            self.sidebar_entries.extend(files);
        }
    }

    pub fn open_file_from_path(&mut self, path: PathBuf) {
        if let Some(existing_idx) = self
            .docs
            .iter()
            .position(|d| d.path.as_deref() == Some(path.as_path()))
        {
            self.active_doc = existing_idx;
            self.save_session();
            return;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            let title = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("file")
                .to_string();
            self.docs.push(Document::new_with_text(&title, Some(path), &text));
            self.active_doc = self.docs.len() - 1;
            self.save_session();
        }
    }

    pub fn zoom_in(&mut self) {
        self.font_size = (self.font_size + 1).min(32);
        self.save_session();
    }

    pub fn zoom_out(&mut self) {
        self.font_size = (self.font_size - 1).max(10);
        self.save_session();
    }

    pub fn zoom_reset(&mut self) {
        self.font_size = 15;
        self.save_session();
    }

    pub fn doc(&self) -> &Document {
        &self.docs[self.active_doc]
    }

    pub fn doc_mut(&mut self) -> &mut Document {
        &mut self.docs[self.active_doc]
    }

    pub fn theme(&self) -> &'static EditorTheme {
        EditorTheme::by_index(self.theme_idx)
    }

    pub fn close_active_tab(&mut self) {
        if self.docs.len() > 1 {
            self.docs.remove(self.active_doc);
            if self.active_doc >= self.docs.len() {
                self.active_doc = self.docs.len() - 1;
            }
        } else {
            self.docs[0] = Document::new_with_text("untitled_1.rs", None, "");
        }
        self.save_session();
    }

    pub fn toggle_vim_mode(&mut self) {
        self.vim_pending_key = None;
        self.vim_mode = if self.vim_mode == VimMode::Off {
            VimMode::Normal
        } else {
            VimMode::Off
        };
        self.save_session();
    }

    pub fn filtered_palette_entries(&self) -> Vec<PaletteEntry> {
        let mut all = vec![
            PaletteEntry {
                label: "Format Document / Code".into(),
                shortcut: "Ctrl+Shift+I",
                action: PaletteAction::FormatDocument,
            },
            PaletteEntry {
                label: "Toggle File Explorer Sidebar".into(),
                shortcut: "Ctrl+B",
                action: PaletteAction::ToggleSidebar,
            },
            PaletteEntry {
                label: "Toggle Code Minimap".into(),
                shortcut: "Ctrl+M",
                action: PaletteAction::ToggleMinimap,
            },
            PaletteEntry {
                label: "Go to Line...".into(),
                shortcut: "Ctrl+G",
                action: PaletteAction::GoToLine,
            },
            PaletteEntry {
                label: "Toggle Bookmark on Line".into(),
                shortcut: "Ctrl+F2",
                action: PaletteAction::ToggleBookmark,
            },
            PaletteEntry {
                label: "Jump to Next Bookmark".into(),
                shortcut: "F2",
                action: PaletteAction::NextBookmark,
            },
            PaletteEntry {
                label: "Zoom In Font Size".into(),
                shortcut: "Ctrl++",
                action: PaletteAction::ZoomIn,
            },
            PaletteEntry {
                label: "Zoom Out Font Size".into(),
                shortcut: "Ctrl+-",
                action: PaletteAction::ZoomOut,
            },
            PaletteEntry {
                label: "Reset Font Zoom (100%)".into(),
                shortcut: "Ctrl+0",
                action: PaletteAction::ZoomReset,
            },
            PaletteEntry {
                label: "Find in Document...".into(),
                shortcut: "Ctrl+F",
                action: PaletteAction::OpenFind,
            },
            PaletteEntry {
                label: "Find and Replace...".into(),
                shortcut: "Ctrl+H",
                action: PaletteAction::OpenReplace,
            },
            PaletteEntry {
                label: "Toggle Line Comment".into(),
                shortcut: "Ctrl+/",
                action: PaletteAction::ToggleComment,
            },
            PaletteEntry {
                label: "Duplicate Line or Selection".into(),
                shortcut: "Ctrl+Shift+D",
                action: PaletteAction::DuplicateLine,
            },
            PaletteEntry {
                label: "Move Line Up".into(),
                shortcut: "Alt+Up",
                action: PaletteAction::MoveLineUp,
            },
            PaletteEntry {
                label: "Move Line Down".into(),
                shortcut: "Alt+Down",
                action: PaletteAction::MoveLineDown,
            },
            PaletteEntry {
                label: "Add Cursor to Next Occurrence".into(),
                shortcut: "Ctrl+D",
                action: PaletteAction::AddCursorNextOccurrence,
            },
            PaletteEntry {
                label: "Toggle Vim Mode (Normal / Insert / Visual)".into(),
                shortcut: "Ctrl+Alt+V",
                action: PaletteAction::ToggleVimMode,
            },
            PaletteEntry {
                label: "Toggle Word Wrap".into(),
                shortcut: "Alt+Z",
                action: PaletteAction::ToggleWordWrap,
            },
            PaletteEntry {
                label: "Toggle Sticky Scroll Headers".into(),
                shortcut: "",
                action: PaletteAction::ToggleStickyScroll,
            },
            PaletteEntry {
                label: "Toggle Inline Color Previews (#rrggbb)".into(),
                shortcut: "",
                action: PaletteAction::ToggleColorPreviews,
            },
            PaletteEntry {
                label: "Toggle Rainbow Bracket Colorization".into(),
                shortcut: "",
                action: PaletteAction::ToggleRainbowBrackets,
            },
            PaletteEntry {
                label: "Toggle Visible Whitespace (· / →)".into(),
                shortcut: "",
                action: PaletteAction::ToggleWhitespace,
            },
            PaletteEntry {
                label: "Toggle Indentation Guides".into(),
                shortcut: "",
                action: PaletteAction::ToggleIndentGuides,
            },
            PaletteEntry {
                label: "Fold / Unfold Block at Cursor".into(),
                shortcut: "",
                action: PaletteAction::ToggleFoldAtCursor,
            },
            PaletteEntry {
                label: "Unfold All Blocks".into(),
                shortcut: "",
                action: PaletteAction::UnfoldAll,
            },
            PaletteEntry {
                label: "File: New Editor Tab".into(),
                shortcut: "Ctrl+N",
                action: PaletteAction::NewTab,
            },
            PaletteEntry {
                label: "File: Close Active File".into(),
                shortcut: "Ctrl+W",
                action: PaletteAction::CloseFile,
            },
            PaletteEntry {
                label: "Indentation: 4 Spaces".into(),
                shortcut: "",
                action: PaletteAction::IndentSpaces4,
            },
            PaletteEntry {
                label: "Indentation: 2 Spaces".into(),
                shortcut: "",
                action: PaletteAction::IndentSpaces2,
            },
            PaletteEntry {
                label: "Indentation: Tab Character".into(),
                shortcut: "",
                action: PaletteAction::IndentTabs,
            },
            PaletteEntry {
                label: "Help: About Developer (Shay Kadosh)".into(),
                shortcut: "F1",
                action: PaletteAction::AboutDeveloper,
            },
        ];

        for (idx, th) in EditorTheme::all().iter().enumerate() {
            all.push(PaletteEntry {
                label: format!("Theme: {}", th.name),
                shortcut: "24 Built-in",
                action: PaletteAction::SetTheme(idx),
            });
        }

        let q = self.palette_query.trim().to_lowercase();
        if q.is_empty() {
            all
        } else {
            all.into_iter()
                .filter(|e| e.label.to_lowercase().contains(&q))
                .collect()
        }
    }

    pub fn execute_palette_action(
        &mut self,
        action: PaletteAction,
        tabs: &mut Widget,
        sb: &mut Frame,
    ) {
        self.overlay = OverlayMode::None;
        match action {
            PaletteAction::FormatDocument => self.doc_mut().format_document(),
            PaletteAction::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
                self.save_session();
            }
            PaletteAction::ToggleMinimap => {
                self.show_minimap = !self.show_minimap;
                self.save_session();
            }
            PaletteAction::ToggleBookmark => self.doc_mut().toggle_bookmark(),
            PaletteAction::NextBookmark => {
                self.doc_mut().next_bookmark();
            }
            PaletteAction::GoToLine => {
                self.goto_query.clear();
                self.overlay = OverlayMode::GoToLine;
            }
            PaletteAction::ZoomIn => self.zoom_in(),
            PaletteAction::ZoomOut => self.zoom_out(),
            PaletteAction::ZoomReset => self.zoom_reset(),
            PaletteAction::ToggleComment => {
                let lang = self
                    .syntax
                    .detect_language_name(self.doc().path.as_deref(), &self.doc().title);
                let prefix = SyntaxEngine::comment_prefix(&lang);
                self.doc_mut().toggle_comment(prefix);
            }
            PaletteAction::DuplicateLine => self.doc_mut().duplicate_lines(),
            PaletteAction::MoveLineUp => self.doc_mut().move_lines_up(),
            PaletteAction::MoveLineDown => self.doc_mut().move_lines_down(),
            PaletteAction::AddCursorNextOccurrence => self.doc_mut().add_cursor_next_occurrence(),
            PaletteAction::ToggleWordWrap => {
                let d = self.doc_mut();
                d.wrap_lines = !d.wrap_lines;
            }
            PaletteAction::ToggleStickyScroll => {
                self.sticky_scroll = !self.sticky_scroll;
                self.save_session();
            }
            PaletteAction::ToggleColorPreviews => {
                self.inline_color_previews = !self.inline_color_previews;
                self.save_session();
            }
            PaletteAction::ToggleRainbowBrackets => {
                self.rainbow_brackets = !self.rainbow_brackets;
                self.save_session();
            }
            PaletteAction::ToggleWhitespace => {
                self.show_whitespace = !self.show_whitespace;
                self.save_session();
            }
            PaletteAction::ToggleIndentGuides => {
                self.show_indent_guides = !self.show_indent_guides;
                self.save_session();
            }
            PaletteAction::ToggleFoldAtCursor => {
                let l = self.doc().cursor.line;
                self.doc_mut().toggle_fold_at(l);
            }
            PaletteAction::UnfoldAll => self.doc_mut().unfold_all(),
            PaletteAction::ToggleVimMode => self.toggle_vim_mode(),
            PaletteAction::OpenFind => {
                self.overlay = OverlayMode::Find {
                    replace: false,
                    focus_replace: false,
                };
            }
            PaletteAction::OpenReplace => {
                self.overlay = OverlayMode::Find {
                    replace: true,
                    focus_replace: false,
                };
            }
            PaletteAction::NewTab => {
                let next = self.docs.len() + 1;
                let title = format!("untitled_{}.rs", next);
                self.docs.push(Document::new_with_text(&title, None, ""));
                self.active_doc = self.docs.len() - 1;
            }
            PaletteAction::CloseFile => self.close_active_tab(),
            PaletteAction::IndentSpaces4 => self.doc_mut().indent_style = IndentStyle::Spaces(4),
            PaletteAction::IndentSpaces2 => self.doc_mut().indent_style = IndentStyle::Spaces(2),
            PaletteAction::IndentTabs => self.doc_mut().indent_style = IndentStyle::Tabs,
            PaletteAction::AboutDeveloper => show_about_developer(),
            PaletteAction::SetTheme(idx) => {
                self.theme_idx = idx;
                let th = self.theme();
                sb.set_color(th.status_bg.to_fltk());
                sb.set_label_color(th.status_fg.to_fltk());
                sb.redraw();
                self.save_session();
            }
        }
        tabs.redraw();
    }
}

pub fn show_about_developer() {
    fltk::dialog::message_title("About Studio Code Editor & Developer");
    fltk::dialog::message_default(
        "Studio Code Editor — High-Performance Native IDE Engine\n\n\
         Lead Architect & Developer: Shay Kadosh\n\
         Copyright © 2000–2026 Shay Kadosh. All Rights Reserved.\n\n\
         Engine Highlights:\n\
         • Custom 2D Hardware-Friendly Canvas Renderer (FLTK 1.5)\n\
         • Multi-Language Syntax Highlighting via Syntect\n\
         • 24 Built-in Studio & Iced Themes + Session Persistence\n\
         • File Explorer Sidebar, Colored Minimap, Bookmarks & Smart Autocomplete\n\
         • Code Folding, Sticky Scroll, Rainbow Brackets & Inline Color Swatches\n\
         • Multi-Cursor Editing, Command Palette (Ctrl+Shift+P) & Optional Vim Mode",
    );
}

fn file_badge_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "rs" => "RS",
        "toml" => "TM",
        "json" => "JS",
        "md" => "MD",
        "py" => "PY",
        "js" | "ts" => "TS",
        "c" | "cpp" | "h" => "C+",
        "sh" => "SH",
        _ => "··",
    }
}

/// Scans a line for `#rrggbb`, `#rgb`, `0xrrggbb`, or `rgb(r, g, b)` color literals.
fn find_color_literals(line: &str) -> Vec<(usize, Rgb)> {
    let mut out = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut i = 0usize;

    while i < n {
        if chars[i] == '#' {
            let hex_len = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_hexdigit())
                .count();
            if hex_len == 6 {
                let s: String = chars[i + 1..i + 7].iter().collect();
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&s[0..2], 16),
                    u8::from_str_radix(&s[2..4], 16),
                    u8::from_str_radix(&s[4..6], 16),
                ) {
                    out.push((i + 7, Rgb(r, g, b)));
                    i += 7;
                    continue;
                }
            } else if hex_len == 3 {
                let s: String = chars[i + 1..i + 4].iter().collect();
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&s[0..1], 16),
                    u8::from_str_radix(&s[1..2], 16),
                    u8::from_str_radix(&s[2..3], 16),
                ) {
                    out.push((i + 4, Rgb(r * 17, g * 17, b * 17)));
                    i += 4;
                    continue;
                }
            }
        }

        if i + 8 <= n && chars[i] == '0' && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
            let hex_len = chars[i + 2..]
                .iter()
                .take_while(|c| c.is_ascii_hexdigit())
                .count();
            if hex_len == 6 {
                let s: String = chars[i + 2..i + 8].iter().collect();
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&s[0..2], 16),
                    u8::from_str_radix(&s[2..4], 16),
                    u8::from_str_radix(&s[4..6], 16),
                ) {
                    out.push((i + 8, Rgb(r, g, b)));
                    i += 8;
                    continue;
                }
            }
        }

        if i + 10 <= n
            && chars[i..i + 4]
                .iter()
                .collect::<String>()
                .eq_ignore_ascii_case("rgb(")
        {
            if let Some(close_rel) = chars[i + 4..].iter().position(|&c| c == ')') {
                let inner: String = chars[i + 4..i + 4 + close_rel].iter().collect();
                let parts: Vec<&str> = inner.split(',').collect();
                if parts.len() == 3 {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[0].trim().parse::<u8>(),
                        parts[1].trim().parse::<u8>(),
                        parts[2].trim().parse::<u8>(),
                    ) {
                        let end_col = i + 4 + close_rel + 1;
                        out.push((end_col, Rgb(r, g, b)));
                        i = end_col;
                        continue;
                    }
                }
            }
        }

        i += 1;
    }

    out
}

pub struct CodeCanvas {
    inner: Widget,
}

widget_extends!(CodeCanvas, Widget, inner);

impl CodeCanvas {
    pub fn new(
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        state: Rc<RefCell<EditorState>>,
        tab_bar: Widget,
        status_bar: Frame,
    ) -> Self {
        let mut inner = Widget::new(x, y, w, h, None);

        // High-performance 2D Canvas Draw Routine
        inner.draw({
            let state = Rc::clone(&state);
            move |wid| {
                let st = state.borrow();
                let doc = st.doc();
                let theme = st.theme();
                let font_sz = st.font_size;
                let line_h = st.line_height();
                let sb_w = st.sidebar_w();
                let mm_w = st.minimap_w();

                let wx = wid.x();
                let wy = wid.y();
                let ww = wid.w();
                let wh = wid.h();

                draw::push_clip(wx, wy, ww, wh);

                // 0. File Explorer Sidebar on Left (if enabled)
                if st.show_sidebar {
                    draw::set_draw_color(theme.tab_bar_bg.to_fltk());
                    draw::draw_rectf(wx, wy, sb_w, wh);
                    draw::set_draw_color(theme.indent_guide.to_fltk());
                    draw::draw_line(wx + sb_w - 1, wy, wx + sb_w - 1, wy + wh);

                    // Workspace Folder Header
                    draw::set_draw_color(theme.gutter_bg.to_fltk());
                    draw::draw_rectf(wx, wy, sb_w - 1, 28);
                    draw::set_font(Font::HelveticaBold, 12);
                    draw::set_draw_color(theme.accent.to_fltk());
                    let ws_name = st
                        .workspace_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("EXPLORER");
                    draw::draw_text2(
                        &format!("  EXPLORER: {}", ws_name.to_uppercase()),
                        wx + 6,
                        wy,
                        sb_w - 12,
                        28,
                        Align::Left | Align::Inside,
                    );

                    // Entries
                    let row_h = 24;
                    let max_entries = ((wh - 30) / row_h).max(1) as usize;
                    for (idx, entry) in st
                        .sidebar_entries
                        .iter()
                        .skip(st.sidebar_scroll)
                        .take(max_entries)
                        .enumerate()
                    {
                        let ey = wy + 30 + (idx as i32) * row_h;
                        let is_open_doc = doc.path.as_deref() == Some(entry.path.as_path());
                        if is_open_doc {
                            draw::set_draw_color(theme.selection_bg.to_fltk());
                            draw::draw_rectf(wx + 4, ey, sb_w - 9, row_h - 1);
                        }

                        draw::set_font(Font::CourierBold, 11);
                        if entry.is_dir {
                            draw::set_draw_color(theme.accent.to_fltk());
                            let icon = if entry.is_parent { "↑" } else { "▸" };
                            draw::draw_text(icon, wx + 10, ey + 16);
                        } else {
                            draw::set_draw_color(theme.gutter_active_fg.to_fltk());
                            draw::draw_text(file_badge_for(&entry.path), wx + 8, ey + 16);
                        }

                        draw::set_font(Font::Helvetica, 13);
                        draw::set_draw_color(if is_open_doc {
                            theme.accent.to_fltk()
                        } else if entry.is_dir {
                            theme.text_fg.to_fltk()
                        } else {
                            theme.status_fg.to_fltk()
                        });
                        draw::draw_text2(
                            &entry.name,
                            wx + 32,
                            ey,
                            sb_w - 38,
                            row_h,
                            Align::Left | Align::Inside,
                        );
                    }
                }

                // Editor canvas coordinates (shifted right by `sb_w`)
                let ed_x = wx + sb_w;
                let ed_w = (ww - sb_w).max(80);

                // 1. Editor background
                draw::set_draw_color(theme.bg.to_fltk());
                draw::draw_rectf(ed_x, wy, ed_w, wh);

                draw::set_font(Font::Courier, font_sz);
                let char_w = draw::width("M").max(7.0) as i32;

                let text_x = ed_x + GUTTER_WIDTH + 10;
                let text_w = (ed_w - GUTTER_WIDTH - mm_w - SCROLLBAR_SIZE - 14).max(40);
                let visible_cols = (text_w / char_w).max(10) as usize;
                let total_rows = (wh / line_h).max(1) as usize;

                // Sticky scroll headers at the top of viewport
                let vis_doc_lines = doc.visible_doc_lines();
                let scroll_vis_idx = doc.scroll_y.min(vis_doc_lines.len().saturating_sub(1));
                let top_doc_line = vis_doc_lines.get(scroll_vis_idx).copied().unwrap_or(0);
                let sticky_lines = if st.sticky_scroll {
                    doc.sticky_headers_for(top_doc_line)
                } else {
                    Vec::new()
                };
                let sticky_count = sticky_lines.len();
                let body_y = wy + (sticky_count as i32) * line_h;
                let body_rows = total_rows.saturating_sub(sticky_count).max(1);

                let end_vis_idx = (scroll_vis_idx + body_rows + 2).min(vis_doc_lines.len());
                let max_doc_line = vis_doc_lines
                    .get(end_vis_idx.saturating_sub(1))
                    .copied()
                    .unwrap_or(0)
                    + 1;
                let min_doc_line = sticky_lines
                    .first()
                    .copied()
                    .unwrap_or(top_doc_line)
                    .min(top_doc_line);

                let highlighted = st.syntax.highlight_lines(
                    &doc.lines,
                    min_doc_line,
                    max_doc_line.min(doc.lines.len()),
                    doc.path.as_deref(),
                    &doc.title,
                    theme,
                );

                // Pre-calculate bracket depth
                let mut line_start_depths = vec![0usize; max_doc_line.min(doc.lines.len()) + 1];
                let mut running_depth = 0usize;
                for (l_idx, l_str) in doc
                    .lines
                    .iter()
                    .enumerate()
                    .take(max_doc_line.min(doc.lines.len()))
                {
                    line_start_depths[l_idx] = running_depth;
                    for ch in l_str.chars() {
                        if matches!(ch, '(' | '[' | '{') {
                            running_depth += 1;
                        } else if matches!(ch, ')' | ']' | '}') {
                            running_depth = running_depth.saturating_sub(1);
                        }
                    }
                }

                // 2. Styled Line Number, Bookmark & Code-Folding Gutter
                draw::set_draw_color(theme.gutter_bg.to_fltk());
                draw::draw_rectf(ed_x, wy, GUTTER_WIDTH, wh);
                draw::set_draw_color(theme.indent_guide.to_fltk());
                draw::draw_line(ed_x + GUTTER_WIDTH, wy, ed_x + GUTTER_WIDTH, wy + wh);

                let sel_range = doc.selection_range();
                let matched_brackets = doc.find_matching_bracket();
                let all_cursors = doc.all_cursors();
                let search_matches = if matches!(st.overlay, OverlayMode::Find { .. })
                    && !st.find_query.is_empty()
                {
                    doc.find_all_matches(&st.find_query)
                } else {
                    Vec::new()
                };

                let mut primary_caret_screen: Option<(i32, i32)> = None;

                // 3. Render Main Body Lines
                let mut screen_row = 0usize;
                for &line_idx in &vis_doc_lines[scroll_vis_idx..end_vis_idx] {
                    if screen_row >= body_rows + 1 {
                        break;
                    }
                    let line_str = &doc.lines[line_idx];
                    let line_chars: Vec<char> = line_str.chars().collect();
                    let empty_spans: Vec<StyledSpan> = Vec::new();
                    let spans = highlighted
                        .get(line_idx.saturating_sub(min_doc_line))
                        .unwrap_or(&empty_spans);

                    let segments: Vec<(usize, usize)> =
                        if doc.wrap_lines && line_chars.len() > visible_cols {
                            let mut segs = Vec::new();
                            let mut c = 0;
                            while c < line_chars.len() {
                                let next_c = (c + visible_cols).min(line_chars.len());
                                segs.push((c, next_c));
                                c = next_c;
                            }
                            segs
                        } else {
                            vec![(
                                doc.scroll_x,
                                (doc.scroll_x + visible_cols + 4).min(line_chars.len()),
                            )]
                        };

                    let color_swatches = if st.inline_color_previews {
                        find_color_literals(line_str)
                    } else {
                        Vec::new()
                    };

                    for (seg_idx, (col_start, col_end)) in segments.iter().copied().enumerate() {
                        let row_y = body_y + (screen_row as i32) * line_h;
                        if row_y >= wy + wh {
                            break;
                        }

                        // Active line highlight
                        if line_idx == doc.cursor.line {
                            draw::set_draw_color(theme.active_line_bg.to_fltk());
                            draw::draw_rectf(
                                ed_x + GUTTER_WIDTH + 1,
                                row_y,
                                ed_w - GUTTER_WIDTH - mm_w - SCROLLBAR_SIZE - 1,
                                line_h,
                            );
                        }

                        // Gutter bookmark diamond, line number & fold indicator
                        if seg_idx == 0 {
                            if doc.bookmarks.contains(&line_idx) {
                                draw::set_font(Font::CourierBold, font_sz - 1);
                                draw::set_draw_color(theme.accent.to_fltk());
                                draw::draw_text2(
                                    "◆",
                                    ed_x + 2,
                                    row_y,
                                    14,
                                    line_h,
                                    Align::Center | Align::Inside,
                                );
                            }

                            let is_active = line_idx == doc.cursor.line;
                            draw::set_font(
                                if is_active { Font::CourierBold } else { Font::Courier },
                                font_sz - 1,
                            );
                            draw::set_draw_color(if is_active {
                                theme.gutter_active_fg.to_fltk()
                            } else {
                                theme.gutter_fg.to_fltk()
                            });
                            let num_str = format!("{:>4}", line_idx + 1);
                            draw::draw_text2(
                                &num_str,
                                ed_x + 12,
                                row_y,
                                GUTTER_WIDTH - 30,
                                line_h,
                                Align::Right | Align::Inside,
                            );

                            if doc.foldable_range(line_idx).is_some() {
                                let is_folded = doc.folded_headers.contains(&line_idx);
                                draw::set_draw_color(if is_folded {
                                    theme.accent.to_fltk()
                                } else {
                                    theme.gutter_fg.to_fltk()
                                });
                                let arrow = if is_folded { "▸" } else { "▾" };
                                draw::draw_text2(
                                    arrow,
                                    ed_x + GUTTER_WIDTH - 16,
                                    row_y,
                                    14,
                                    line_h,
                                    Align::Center | Align::Inside,
                                );
                            }
                            draw::set_font(Font::Courier, font_sz);
                        }

                        // Search match highlights
                        for &(m_start, m_end) in &search_matches {
                            if m_start.line == line_idx {
                                let vis_c0 = m_start.col.max(col_start);
                                let vis_c1 = m_end.col.min(col_end);
                                if vis_c1 > vis_c0 {
                                    let sx = text_x + ((vis_c0 - col_start) as i32) * char_w;
                                    let sw = ((vis_c1 - vis_c0) as i32) * char_w;
                                    draw::set_draw_color(theme.bracket_match_bg.to_fltk());
                                    draw::draw_rectf(sx, row_y + 2, sw, line_h - 4);
                                    draw::set_draw_color(theme.accent.to_fltk());
                                    draw::draw_rect(sx, row_y + 2, sw, line_h - 4);
                                }
                            }
                        }

                        // Selection highlight
                        if let Some((s_start, s_end)) = sel_range {
                            if line_idx >= s_start.line && line_idx <= s_end.line {
                                let sel_c0 = if line_idx == s_start.line { s_start.col } else { 0 };
                                let sel_c1 = if line_idx == s_end.line {
                                    s_end.col
                                } else {
                                    line_chars.len() + 1
                                };
                                let vis_c0 = sel_c0.max(col_start);
                                let vis_c1 = sel_c1.min(col_end.max(col_start + 1));
                                if vis_c1 > vis_c0 {
                                    let sx = text_x + ((vis_c0 - col_start) as i32) * char_w;
                                    let sw = ((vis_c1 - vis_c0) as i32) * char_w;
                                    draw::set_draw_color(theme.selection_bg.to_fltk());
                                    draw::draw_rectf(sx, row_y, sw, line_h);
                                }
                            }
                        }

                        // Indentation guides
                        if st.show_indent_guides && seg_idx == 0 && col_start == 0 {
                            let leading_spaces =
                                line_chars.iter().take_while(|&&c| c == ' ').count();
                            let step = doc.indent_style.step_cols();
                            draw::set_draw_color(theme.indent_guide.to_fltk());
                            let mut g_col = step;
                            while g_col < leading_spaces {
                                let gx = text_x + (g_col as i32) * char_w;
                                draw::draw_line(gx, row_y + 2, gx, row_y + line_h - 2);
                                g_col += step;
                            }
                        }

                        // Matching bracket/quote highlight boxes
                        if let Some((b1, b2)) = matched_brackets {
                            for bp in [b1, b2] {
                                if bp.line == line_idx && bp.col >= col_start && bp.col < col_end {
                                    let bx = text_x + ((bp.col - col_start) as i32) * char_w;
                                    draw::set_draw_color(theme.bracket_match_bg.to_fltk());
                                    draw::draw_rectf(bx - 1, row_y + 2, char_w + 2, line_h - 4);
                                    draw::set_draw_color(theme.accent.to_fltk());
                                    draw::draw_rect(bx - 1, row_y + 2, char_w + 2, line_h - 4);
                                }
                            }
                        }

                        // Build per-character colors from Syntect spans
                        let mut char_colors: Vec<Rgb> = Vec::with_capacity(line_chars.len());
                        for sp in spans {
                            for _ in sp.text.chars() {
                                char_colors.push(sp.fg);
                            }
                        }
                        while char_colors.len() < line_chars.len() {
                            char_colors.push(theme.text_fg);
                        }

                        let mut bracket_depth = line_start_depths
                            .get(line_idx)
                            .copied()
                            .unwrap_or(0);
                        for &c in line_chars.iter().take(col_start) {
                            if matches!(c, '(' | '[' | '{') {
                                bracket_depth += 1;
                            } else if matches!(c, ')' | ']' | '}') {
                                bracket_depth = bracket_depth.saturating_sub(1);
                            }
                        }

                        // Draw visible characters
                        let mut col_idx = col_start;
                        while col_idx < col_end && col_idx < line_chars.len() {
                            let ch = line_chars[col_idx];
                            let cx = text_x + ((col_idx - col_start) as i32) * char_w;
                            let baseline_y = row_y + line_h - 5;

                            if ch == ' ' || ch == '\t' {
                                if st.show_whitespace {
                                    draw::set_draw_color(theme.gutter_fg.to_fltk());
                                    let sym = if ch == ' ' { "·" } else { "→" };
                                    draw::draw_text(sym, cx, baseline_y);
                                }
                                col_idx += 1;
                                continue;
                            }

                            let fg = if st.rainbow_brackets && matches!(ch, '(' | '[' | '{') {
                                let c = theme.rainbow[bracket_depth % theme.rainbow.len()];
                                bracket_depth += 1;
                                c
                            } else if st.rainbow_brackets && matches!(ch, ')' | ']' | '}') {
                                bracket_depth = bracket_depth.saturating_sub(1);
                                theme.rainbow[bracket_depth % theme.rainbow.len()]
                            } else {
                                char_colors.get(col_idx).copied().unwrap_or(theme.text_fg)
                            };
                            draw::set_draw_color(fg.to_fltk());
                            let mut buf = [0u8; 4];
                            let s = ch.encode_utf8(&mut buf);
                            draw::draw_text(s, cx, baseline_y);
                            col_idx += 1;
                        }

                        // Inline Color Previews
                        if seg_idx + 1 == segments.len() && !color_swatches.is_empty() {
                            let end_x =
                                text_x + ((col_end.saturating_sub(col_start)) as i32) * char_w + 10;
                            for (swatch_idx, &(_, rgb)) in color_swatches.iter().enumerate() {
                                let sx = end_x + (swatch_idx as i32) * 18;
                                if sx + 14 < ed_x + ed_w - mm_w - SCROLLBAR_SIZE {
                                    let sy = row_y + (line_h - 12) / 2;
                                    draw::set_draw_color(rgb.to_fltk());
                                    draw::draw_rectf(sx, sy, 12, 12);
                                    draw::set_draw_color(Color::White);
                                    draw::draw_rect(sx, sy, 12, 12);
                                }
                            }
                        }

                        // Folded block badge `⋯ [N lines]`
                        if seg_idx + 1 == segments.len() && doc.folded_headers.contains(&line_idx) {
                            if let Some(end_fold) = doc.foldable_range(line_idx) {
                                let hidden = end_fold - line_idx;
                                let badge = format!(" ⋯ [{} lines] ", hidden);
                                let bx = text_x
                                    + ((col_end.saturating_sub(col_start)) as i32) * char_w
                                    + 8;
                                let bw = (badge.chars().count() as i32) * char_w;
                                draw::set_draw_color(theme.selection_bg.to_fltk());
                                draw::draw_rectf(bx, row_y + 2, bw, line_h - 4);
                                draw::set_draw_color(theme.accent.to_fltk());
                                draw::draw_text(&badge, bx, row_y + line_h - 5);
                            }
                        }

                        // Draw all active cursors
                        for &cur_pos in &all_cursors {
                            if cur_pos.line == line_idx {
                                let cur_c = cur_pos.col;
                                let in_seg = (cur_c >= col_start && cur_c < col_end)
                                    || (seg_idx + 1 == segments.len() && cur_c == col_end);
                                if in_seg && cur_c >= col_start {
                                    let caret_x = text_x + ((cur_c - col_start) as i32) * char_w;
                                    let is_primary = cur_pos == doc.cursor;
                                    if is_primary {
                                        primary_caret_screen = Some((caret_x, row_y + line_h));
                                    }
                                    draw::set_draw_color(if is_primary {
                                        theme.cursor.to_fltk()
                                    } else {
                                        theme.accent.to_fltk()
                                    });
                                    let is_block = matches!(
                                        st.vim_mode,
                                        VimMode::Normal | VimMode::Visual | VimMode::VisualLine
                                    );
                                    if is_block {
                                        draw::draw_rect(caret_x, row_y + 2, char_w, line_h - 4);
                                        draw::draw_rectf(caret_x, row_y + line_h - 4, char_w, 2);
                                    } else {
                                        draw::draw_rectf(caret_x, row_y + 2, 2, line_h - 4);
                                    }
                                }
                            }
                        }

                        screen_row += 1;
                    }
                }

                // 4. Sticky Scroll Header Overlay
                if !sticky_lines.is_empty() {
                    let sticky_h = (sticky_lines.len() as i32) * line_h;
                    draw::set_draw_color(theme.gutter_bg.to_fltk());
                    draw::draw_rectf(ed_x, wy, ed_w - mm_w - SCROLLBAR_SIZE, sticky_h);

                    for (s_row, &h_line) in sticky_lines.iter().enumerate() {
                        let ry = wy + (s_row as i32) * line_h;
                        draw::set_font(Font::CourierBold, font_sz - 1);
                        draw::set_draw_color(theme.accent.to_fltk());
                        let num_str = format!("{:>4}", h_line + 1);
                        draw::draw_text2(
                            &num_str,
                            ed_x + 12,
                            ry,
                            GUTTER_WIDTH - 30,
                            line_h,
                            Align::Right | Align::Inside,
                        );

                        draw::set_font(Font::Courier, font_sz);
                        draw::set_draw_color(theme.text_fg.to_fltk());
                        let header_text = &doc.lines[h_line];
                        draw::draw_text(header_text, text_x, ry + line_h - 5);
                    }

                    draw::set_draw_color(theme.accent.to_fltk());
                    draw::draw_line(
                        ed_x,
                        wy + sticky_h - 1,
                        ed_x + ed_w - mm_w - SCROLLBAR_SIZE,
                        wy + sticky_h - 1,
                    );
                }

                // 5. Colored Code Minimap on the Right (between editor text and scrollbar)
                if st.show_minimap {
                    let mm_x = ed_x + ed_w - SCROLLBAR_SIZE - mm_w;
                    draw::set_draw_color(theme.gutter_bg.to_fltk());
                    draw::draw_rectf(mm_x, wy, mm_w, wh);
                    draw::set_draw_color(theme.indent_guide.to_fltk());
                    draw::draw_line(mm_x, wy, mm_x, wy + wh);

                    let max_mm_lines = (wh / 3).max(1) as usize;
                    let total_doc = doc.lines.len().max(1);
                    let mm_start = if total_doc > max_mm_lines {
                        let center = top_doc_line;
                        center.saturating_sub(max_mm_lines / 2).min(total_doc - max_mm_lines)
                    } else {
                        0
                    };
                    let mm_end = (mm_start + max_mm_lines).min(total_doc);

                    // Viewport slider box on minimap
                    let vp_start_row = top_doc_line.saturating_sub(mm_start) as i32;
                    let vp_y = wy + vp_start_row * 3;
                    let vp_h = ((body_rows as i32) * 3).clamp(16, wh);
                    draw::set_draw_color(theme.selection_bg.to_fltk());
                    draw::draw_rectf(mm_x + 1, vp_y, mm_w - 2, vp_h);
                    draw::set_draw_color(theme.accent.to_fltk());
                    draw::draw_rect(mm_x + 1, vp_y, mm_w - 2, vp_h);

                    for (m_row, l_idx) in (mm_start..mm_end).enumerate() {
                        let my_row = wy + (m_row as i32) * 3 + 1;
                        if my_row >= wy + wh - 2 {
                            break;
                        }
                        if doc.bookmarks.contains(&l_idx) {
                            draw::set_draw_color(theme.accent.to_fltk());
                            draw::draw_rectf(mm_x + 2, my_row, 4, 2);
                        }
                        let l_str = &doc.lines[l_idx];
                        let trimmed = l_str.trim_start();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let indent = (l_str.len() - trimmed.len()).min(24) as i32;
                        let bar_len = (trimmed.len().min(48) as i32).max(3);
                        let color = if trimmed.starts_with("//") || trimmed.starts_with('#') {
                            theme.gutter_fg
                        } else if trimmed.starts_with("pub ")
                            || trimmed.starts_with("fn ")
                            || trimmed.starts_with("impl")
                            || trimmed.starts_with("struct")
                        {
                            theme.accent
                        } else {
                            theme.text_fg
                        };
                        draw::set_draw_color(color.to_fltk());
                        let bx = (mm_x + 6 + indent).min(mm_x + mm_w - 6);
                        let bw = bar_len.min(mm_x + mm_w - 3 - bx).max(2);
                        draw::draw_rectf(bx, my_row, bw, 2);
                    }
                }

                // 6. Custom Themed Vertical Scrollbar
                let sb_x = ed_x + ed_w - SCROLLBAR_SIZE;
                draw::set_draw_color(theme.scrollbar_track.to_fltk());
                draw::draw_rectf(sb_x, wy, SCROLLBAR_SIZE, wh);

                let total_vis = vis_doc_lines.len().max(1);
                let thumb_h = ((body_rows as f32 / total_vis as f32) * (wh as f32))
                    .clamp(28.0, wh as f32) as i32;
                let max_scroll = total_vis.saturating_sub(1).max(1);
                let scroll_ratio = (scroll_vis_idx as f32 / max_scroll as f32).clamp(0.0, 1.0);
                let thumb_y = wy + ((wh - thumb_h) as f32 * scroll_ratio) as i32;

                let thumb_col = if st.hover_vscroll || st.dragging_vscroll.is_some() {
                    theme.scrollbar_hover
                } else {
                    theme.scrollbar_thumb
                };
                draw::set_draw_color(thumb_col.to_fltk());
                draw::draw_rectf(sb_x + 2, thumb_y, SCROLLBAR_SIZE - 4, thumb_h);

                // 7. Smart Autocomplete Popup (below primary cursor while typing)
                if st.autocomplete_open && st.overlay == OverlayMode::None {
                    let candidates = doc.completion_candidates();
                    if !candidates.is_empty() {
                        if let Some((cx, cy)) = primary_caret_screen {
                            let pop_w = 220;
                            let row_h = 22;
                            let pop_h = (candidates.len() as i32) * row_h + 6;
                            let px = cx.min(ed_x + ed_w - pop_w - 20);
                            let py = if cy + pop_h < wy + wh {
                                cy + 2
                            } else {
                                (cy - line_h - pop_h).max(wy + 4)
                            };

                            draw::set_draw_color(theme.gutter_bg.to_fltk());
                            draw::draw_rectf(px, py, pop_w, pop_h);
                            draw::set_draw_color(theme.accent.to_fltk());
                            draw::draw_rect(px, py, pop_w, pop_h);

                            let sel = st
                                .autocomplete_selected
                                .min(candidates.len().saturating_sub(1));
                            draw::set_font(Font::Courier, 13);
                            for (i, word) in candidates.iter().enumerate() {
                                let iy = py + 3 + (i as i32) * row_h;
                                if i == sel {
                                    draw::set_draw_color(theme.selection_bg.to_fltk());
                                    draw::draw_rectf(px + 2, iy, pop_w - 4, row_h);
                                    draw::set_draw_color(theme.accent.to_fltk());
                                } else {
                                    draw::set_draw_color(theme.text_fg.to_fltk());
                                }
                                draw::draw_text2(
                                    &format!("  {}  [Tab]", word),
                                    px + 4,
                                    iy,
                                    pop_w - 8,
                                    row_h,
                                    Align::Left | Align::Inside,
                                );
                            }
                        }
                    }
                }

                // 8. Interactive Find & Replace / Go To Line Overlay Bar
                match st.overlay {
                    OverlayMode::Find {
                        replace,
                        focus_replace,
                    } => {
                        let bar_w = 430.min(ed_w - 40);
                        let bar_h = if replace { 74 } else { 40 };
                        let bx = ed_x + ed_w - mm_w - SCROLLBAR_SIZE - bar_w - 12;
                        let by = wy + 8;

                        draw::set_draw_color(theme.gutter_bg.to_fltk());
                        draw::draw_rectf(bx, by, bar_w, bar_h);
                        draw::set_draw_color(theme.accent.to_fltk());
                        draw::draw_rect(bx, by, bar_w, bar_h);

                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(theme.text_fg.to_fltk());
                        let count_str = format!("{} matches", search_matches.len());
                        let find_box = format!(
                            "Find: {}{}   ({})  [Enter: Next | Esc: Close]",
                            st.find_query,
                            if !focus_replace { "▏" } else { "" },
                            count_str
                        );
                        draw::draw_text2(
                            &find_box,
                            bx + 10,
                            by + 6,
                            bar_w - 20,
                            26,
                            Align::Left | Align::Inside,
                        );

                        if replace {
                            let rep_box = format!(
                                "Replace: {}{}   [Tab: Switch | Ctrl+Enter: Replace All]",
                                st.replace_query,
                                if focus_replace { "▏" } else { "" }
                            );
                            draw::draw_text2(
                                &rep_box,
                                bx + 10,
                                by + 38,
                                bar_w - 20,
                                26,
                                Align::Left | Align::Inside,
                            );
                        }
                    }
                    OverlayMode::GoToLine => {
                        let bar_w = 340.min(ed_w - 40);
                        let bar_h = 40;
                        let bx = ed_x + (ed_w - bar_w) / 2;
                        let by = wy + 12;

                        draw::set_draw_color(theme.gutter_bg.to_fltk());
                        draw::draw_rectf(bx, by, bar_w, bar_h);
                        draw::set_draw_color(theme.accent.to_fltk());
                        draw::draw_rect(bx, by, bar_w, bar_h);

                        draw::set_font(Font::HelveticaBold, 13);
                        draw::set_draw_color(theme.text_fg.to_fltk());
                        let prompt = format!(
                            "Go to Line (1–{}): {}▏  [Enter]",
                            doc.lines.len(),
                            st.goto_query
                        );
                        draw::draw_text2(
                            &prompt,
                            bx + 12,
                            by + 6,
                            bar_w - 24,
                            28,
                            Align::Left | Align::Inside,
                        );
                    }
                    OverlayMode::CommandPalette => {
                        let entries = st.filtered_palette_entries();
                        let pal_w = 540.min(ed_w - 40);
                        let max_items = 10usize;
                        let shown = entries.len().min(max_items);
                        let pal_h = 44 + (shown as i32) * 28 + 8;
                        let px = ed_x + (ed_w - pal_w) / 2;
                        let py = wy + 14;

                        draw::set_draw_color(theme.gutter_bg.to_fltk());
                        draw::draw_rectf(px, py, pal_w, pal_h);
                        draw::set_draw_color(theme.accent.to_fltk());
                        draw::draw_rect(px, py, pal_w, pal_h);

                        draw::set_draw_color(theme.bg.to_fltk());
                        draw::draw_rectf(px + 8, py + 8, pal_w - 16, 28);
                        draw::set_font(Font::HelveticaBold, 13);
                        draw::set_draw_color(theme.accent.to_fltk());
                        let prompt = format!("> {}▏", st.palette_query);
                        draw::draw_text2(
                            &prompt,
                            px + 14,
                            py + 8,
                            pal_w - 28,
                            28,
                            Align::Left | Align::Inside,
                        );

                        let sel_idx = st.palette_selected.min(entries.len().saturating_sub(1));
                        let start_item = sel_idx.saturating_sub(max_items - 1);

                        for (row, entry) in entries
                            .iter()
                            .skip(start_item)
                            .take(max_items)
                            .enumerate()
                        {
                            let item_idx = start_item + row;
                            let iy = py + 42 + (row as i32) * 28;
                            let is_sel = item_idx == sel_idx;

                            if is_sel {
                                draw::set_draw_color(theme.selection_bg.to_fltk());
                                draw::draw_rectf(px + 6, iy, pal_w - 12, 26);
                            }

                            draw::set_font(Font::Helvetica, 13);
                            draw::set_draw_color(if is_sel {
                                theme.accent.to_fltk()
                            } else {
                                theme.text_fg.to_fltk()
                            });
                            draw::draw_text2(
                                &entry.label,
                                px + 16,
                                iy,
                                pal_w - 150,
                                26,
                                Align::Left | Align::Inside,
                            );

                            if !entry.shortcut.is_empty() {
                                draw::set_draw_color(theme.gutter_fg.to_fltk());
                                draw::draw_text2(
                                    entry.shortcut,
                                    px + pal_w - 135,
                                    iy,
                                    120,
                                    26,
                                    Align::Right | Align::Inside,
                                );
                            }
                        }
                    }
                    OverlayMode::None => {}
                }

                draw::pop_clip();
            }
        });

        // Mouse & Keyboard Event Handling
        inner.handle({
            let state = Rc::clone(&state);
            let mut tabs = tab_bar;
            let mut sb = status_bar;
            move |wid, ev| match ev {
                Event::Focus | Event::Unfocus => {
                    wid.redraw();
                    true
                }
                Event::Enter => {
                    wid.window().unwrap().set_cursor(Cursor::Insert);
                    true
                }
                Event::Leave => {
                    state.borrow_mut().hover_vscroll = false;
                    if let Some(mut w) = wid.window() {
                        w.set_cursor(Cursor::Default);
                    }
                    wid.redraw();
                    true
                }
                Event::Move => {
                    let mx = app::event_x();
                    let sb_w = state.borrow().sidebar_w();
                    let mm_w = state.borrow().minimap_w();
                    let on_sb = mx >= wid.x() + wid.w() - SCROLLBAR_SIZE;
                    let on_mm = mx >= wid.x() + wid.w() - SCROLLBAR_SIZE - mm_w && !on_sb;
                    let on_left_panel = mx < wid.x() + sb_w + GUTTER_WIDTH;
                    let mut st = state.borrow_mut();
                    if st.hover_vscroll != on_sb {
                        st.hover_vscroll = on_sb;
                        wid.redraw();
                    }
                    if let Some(mut w) = wid.window() {
                        w.set_cursor(if on_sb || on_mm || on_left_panel {
                            Cursor::Hand
                        } else {
                            Cursor::Insert
                        });
                    }
                    true
                }
                Event::Push => {
                    let _ = wid.take_focus();
                    let mx = app::event_x();
                    let my = app::event_y();

                    // Right-Click Context Menu (`app::event_button() == 3`)
                    if app::event_button() == 3 {
                        Self::show_context_menu(wid, &state, &mut tabs, &mut sb, mx, my);
                        return true;
                    }

                    let sb_w = state.borrow().sidebar_w();
                    let mm_w = state.borrow().minimap_w();
                    let ed_x = wid.x() + sb_w;
                    let ed_w = (wid.w() - sb_w).max(80);

                    // 0. Click inside File Explorer Sidebar
                    if state.borrow().show_sidebar && mx < ed_x {
                        let rel_y = my - (wid.y() + 30);
                        if rel_y >= 0 {
                            let row = (rel_y / 24) as usize;
                            let mut st = state.borrow_mut();
                            let idx = st.sidebar_scroll + row;
                            if let Some(entry) = st.sidebar_entries.get(idx).cloned() {
                                if entry.is_dir {
                                    st.workspace_dir = entry.path;
                                    st.sidebar_scroll = 0;
                                    st.refresh_sidebar();
                                    st.save_session();
                                } else {
                                    st.open_file_from_path(entry.path);
                                }
                            }
                            drop(st);
                            Self::update_status(&state.borrow(), &mut sb);
                            tabs.redraw();
                            wid.redraw();
                        }
                        return true;
                    }

                    // Click inside Command Palette overlay
                    if state.borrow().overlay == OverlayMode::CommandPalette {
                        let pal_w = 540.min(ed_w - 40);
                        let px = ed_x + (ed_w - pal_w) / 2;
                        let py = wid.y() + 14;
                        let entries = state.borrow().filtered_palette_entries();
                        let max_items = 10usize;
                        let sel_idx = state
                            .borrow()
                            .palette_selected
                            .min(entries.len().saturating_sub(1));
                        let start_item = sel_idx.saturating_sub(max_items - 1);

                        if mx >= px && mx <= px + pal_w && my >= py + 42 {
                            let row = ((my - (py + 42)) / 28) as usize;
                            if let Some(entry) = entries.get(start_item + row) {
                                let act = entry.action;
                                state
                                    .borrow_mut()
                                    .execute_palette_action(act, &mut tabs, &mut sb);
                                Self::update_status(&state.borrow(), &mut sb);
                                wid.redraw();
                                return true;
                            }
                        } else if mx < px || mx > px + pal_w || my < py {
                            state.borrow_mut().overlay = OverlayMode::None;
                            wid.redraw();
                            return true;
                        }
                    }

                    // 1. Vertical scrollbar drag check
                    if mx >= wid.x() + wid.w() - SCROLLBAR_SIZE {
                        let mut st = state.borrow_mut();
                        let cur_scroll = st.doc().scroll_y;
                        st.dragging_vscroll = Some((my, cur_scroll));
                        wid.redraw();
                        return true;
                    }

                    // 1b. Minimap click/drag check
                    if state.borrow().show_minimap
                        && mx >= wid.x() + wid.w() - SCROLLBAR_SIZE - mm_w
                    {
                        let mut st = state.borrow_mut();
                        st.dragging_minimap = true;
                        let ratio = ((my - wid.y()) as f32 / wid.h().max(1) as f32).clamp(0.0, 1.0);
                        let total = st.doc().visible_doc_lines().len().saturating_sub(1);
                        st.doc_mut().scroll_y = ((total as f32) * ratio) as usize;
                        wid.redraw();
                        return true;
                    }

                    // 2. Sticky Scroll click check
                    {
                        let mut st = state.borrow_mut();
                        let line_h = st.line_height();
                        let sticky_enabled = st.sticky_scroll;
                        let vis_lines = st.doc().visible_doc_lines();
                        let scroll_idx = st.doc().scroll_y.min(vis_lines.len().saturating_sub(1));
                        let top_line = vis_lines.get(scroll_idx).copied().unwrap_or(0);
                        let sticky = if sticky_enabled {
                            st.doc().sticky_headers_for(top_line)
                        } else {
                            Vec::new()
                        };
                        let clicked_row = ((my - wid.y()).max(0) / line_h) as usize;
                        if clicked_row < sticky.len() {
                            let target_doc_line = sticky[clicked_row];
                            let target_vis_idx = vis_lines
                                .iter()
                                .position(|&l| l == target_doc_line)
                                .unwrap_or(0);
                            let doc = st.doc_mut();
                            doc.scroll_y = target_vis_idx;
                            doc.cursor = Pos::new(target_doc_line, 0);
                            doc.anchor = None;
                            drop(st);
                            Self::update_status(&state.borrow(), &mut sb);
                            wid.redraw();
                            return true;
                        }
                    }

                    // 3. Gutter click check: toggle bookmark (left edge) or code folding (right edge)
                    let pos = Self::mouse_to_pos(wid, &state.borrow(), mx, my);
                    if mx < ed_x + GUTTER_WIDTH {
                        let mut st = state.borrow_mut();
                        if mx < ed_x + 18 {
                            let l = pos.line;
                            if !st.doc_mut().bookmarks.remove(&l) {
                                st.doc_mut().bookmarks.insert(l);
                            }
                        } else if !st.doc_mut().toggle_fold_at(pos.line) {
                            let l = pos.line;
                            if !st.doc_mut().bookmarks.remove(&l) {
                                st.doc_mut().bookmarks.insert(l);
                            }
                        }
                        drop(st);
                        Self::update_status(&state.borrow(), &mut sb);
                        wid.redraw();
                        return true;
                    }

                    let mods = app::event_state();
                    let shift = mods.contains(Shortcut::Shift);
                    let alt = mods.contains(Shortcut::Alt);
                    let clicks = app::event_clicks();

                    {
                        let mut st = state.borrow_mut();
                        st.autocomplete_open = false;
                        let doc = st.doc_mut();
                        if alt {
                            doc.toggle_extra_cursor(pos);
                        } else if clicks {
                            doc.clear_extra_cursors();
                            doc.select_word_at(pos);
                        } else {
                            doc.clear_extra_cursors();
                            if shift {
                                if doc.anchor.is_none() {
                                    doc.anchor = Some(doc.cursor);
                                }
                            } else {
                                doc.anchor = Some(pos);
                            }
                            doc.cursor = pos;
                            doc.preferred_col = pos.col;
                        }
                    }
                    Self::update_status(&state.borrow(), &mut sb);
                    wid.redraw();
                    true
                }
                Event::Drag => {
                    let mx = app::event_x();
                    let my = app::event_y();

                    if state.borrow().dragging_minimap {
                        let mut st = state.borrow_mut();
                        let ratio = ((my - wid.y()) as f32 / wid.h().max(1) as f32).clamp(0.0, 1.0);
                        let total = st.doc().visible_doc_lines().len().saturating_sub(1);
                        st.doc_mut().scroll_y = ((total as f32) * ratio) as usize;
                        wid.redraw();
                        return true;
                    }

                    let vscroll_drag = state.borrow().dragging_vscroll;
                    if let Some((start_y, start_scroll)) = vscroll_drag {
                        let dy = my - start_y;
                        let mut st = state.borrow_mut();
                        let total = st.doc().visible_doc_lines().len().max(1);
                        let delta_lines =
                            ((dy as f32 / wid.h().max(1) as f32) * total as f32) as isize;
                        let new_scroll = (start_scroll as isize + delta_lines)
                            .clamp(0, total.saturating_sub(1) as isize)
                            as usize;
                        st.doc_mut().scroll_y = new_scroll;
                        wid.redraw();
                        return true;
                    }

                    let pos = Self::mouse_to_pos(wid, &state.borrow(), mx, my);
                    {
                        let mut st = state.borrow_mut();
                        let doc = st.doc_mut();
                        if doc.anchor.is_none() {
                            doc.anchor = Some(doc.cursor);
                        }
                        doc.cursor = pos;
                        doc.preferred_col = pos.col;
                    }
                    Self::update_status(&state.borrow(), &mut sb);
                    wid.redraw();
                    true
                }
                Event::Released => {
                    let mut st = state.borrow_mut();
                    st.dragging_vscroll = None;
                    st.dragging_minimap = false;
                    let doc = st.doc_mut();
                    if doc.anchor == Some(doc.cursor) {
                        doc.anchor = None;
                    }
                    true
                }
                Event::MouseWheel => {
                    let dy = app::event_dy();
                    let ctrl = app::event_state().contains(Shortcut::Ctrl);
                    let mut st = state.borrow_mut();

                    // Ctrl + MouseWheel zooms font size smoothly!
                    if ctrl {
                        match dy {
                            app::MouseWheel::Up => st.zoom_in(),
                            app::MouseWheel::Down => st.zoom_out(),
                            _ => {}
                        }
                        drop(st);
                        Self::update_status(&state.borrow(), &mut sb);
                        wid.redraw();
                        return true;
                    }

                    let max_scroll = st.doc().visible_doc_lines().len().saturating_sub(1);
                    let doc = st.doc_mut();
                    match dy {
                        app::MouseWheel::Down => {
                            doc.scroll_y = (doc.scroll_y + 3).min(max_scroll);
                        }
                        app::MouseWheel::Up => {
                            doc.scroll_y = doc.scroll_y.saturating_sub(3);
                        }
                        _ => {}
                    }
                    wid.redraw();
                    true
                }
                Event::KeyDown => {
                    let key = app::event_key();
                    let state_mods = app::event_state();
                    let ctrl = state_mods.contains(Shortcut::Ctrl);
                    let shift = state_mods.contains(Shortcut::Shift);
                    let alt = state_mods.contains(Shortcut::Alt);

                    let (font_sz, line_h, sb_w, mm_w) = {
                        let st = state.borrow();
                        (st.font_size, st.line_height(), st.sidebar_w(), st.minimap_w())
                    };
                    draw::set_font(Font::Courier, font_sz);
                    let char_w = draw::width("M").max(7.0) as i32;
                    let vis_rows = (wid.h() / line_h).max(1) as usize;
                    let vis_cols = ((wid.w() - sb_w - GUTTER_WIDTH - mm_w - SCROLLBAR_SIZE - 14)
                        / char_w)
                        .max(10) as usize;

                    // Global hotkeys (`Ctrl+Shift+P`, `Ctrl+B`, `Ctrl+M`, `Ctrl+G`, `Ctrl+F`, `Ctrl+H`, `Ctrl+Alt+V`, `F2`)
                    if ctrl && shift && matches!(key.bits() as u8 as char, 'p' | 'P') {
                        let mut st = state.borrow_mut();
                        st.overlay = OverlayMode::CommandPalette;
                        st.palette_query.clear();
                        st.palette_selected = 0;
                        wid.redraw();
                        return true;
                    }
                    if ctrl && !shift && !alt && matches!(key.bits() as u8 as char, 'b' | 'B') {
                        let mut st = state.borrow_mut();
                        st.show_sidebar = !st.show_sidebar;
                        st.save_session();
                        wid.redraw();
                        return true;
                    }
                    if ctrl && !shift && !alt && matches!(key.bits() as u8 as char, 'm' | 'M') {
                        let mut st = state.borrow_mut();
                        st.show_minimap = !st.show_minimap;
                        st.save_session();
                        wid.redraw();
                        return true;
                    }
                    if ctrl && !shift && !alt && matches!(key.bits() as u8 as char, 'g' | 'G') {
                        let mut st = state.borrow_mut();
                        st.goto_query.clear();
                        st.overlay = OverlayMode::GoToLine;
                        wid.redraw();
                        return true;
                    }
                    if key == Key::F2 {
                        let mut st = state.borrow_mut();
                        if ctrl {
                            st.doc_mut().toggle_bookmark();
                        } else {
                            st.doc_mut().next_bookmark();
                            st.doc_mut().ensure_cursor_visible(vis_rows, vis_cols);
                        }
                        drop(st);
                        Self::update_status(&state.borrow(), &mut sb);
                        wid.redraw();
                        return true;
                    }
                    if ctrl && matches!(key.bits() as u8 as char, '=' | '+') {
                        state.borrow_mut().zoom_in();
                        Self::update_status(&state.borrow(), &mut sb);
                        wid.redraw();
                        return true;
                    }
                    if ctrl && matches!(key.bits() as u8 as char, '-') {
                        state.borrow_mut().zoom_out();
                        Self::update_status(&state.borrow(), &mut sb);
                        wid.redraw();
                        return true;
                    }
                    if ctrl && matches!(key.bits() as u8 as char, '0') {
                        state.borrow_mut().zoom_reset();
                        Self::update_status(&state.borrow(), &mut sb);
                        wid.redraw();
                        return true;
                    }
                    if ctrl && !shift && !alt && matches!(key.bits() as u8 as char, 'f' | 'F') {
                        let mut st = state.borrow_mut();
                        if let Some(sel) = st.doc().selected_text() {
                            if !sel.contains('\n') {
                                st.find_query = sel;
                            }
                        }
                        st.overlay = OverlayMode::Find {
                            replace: false,
                            focus_replace: false,
                        };
                        wid.redraw();
                        return true;
                    }
                    if ctrl && !shift && !alt && matches!(key.bits() as u8 as char, 'h' | 'H') {
                        let mut st = state.borrow_mut();
                        if let Some(sel) = st.doc().selected_text() {
                            if !sel.contains('\n') {
                                st.find_query = sel;
                            }
                        }
                        st.overlay = OverlayMode::Find {
                            replace: true,
                            focus_replace: false,
                        };
                        wid.redraw();
                        return true;
                    }
                    if ctrl && alt && matches!(key.bits() as u8 as char, 'v' | 'V') {
                        state.borrow_mut().toggle_vim_mode();
                        Self::update_status(&state.borrow(), &mut sb);
                        wid.redraw();
                        return true;
                    }

                    // 1. Handle Command Palette Keyboard Navigation if active
                    if state.borrow().overlay == OverlayMode::CommandPalette {
                        let mut st = state.borrow_mut();
                        let entries = st.filtered_palette_entries();
                        match key {
                            Key::Escape => {
                                st.overlay = OverlayMode::None;
                            }
                            Key::Up => {
                                st.palette_selected = st.palette_selected.saturating_sub(1);
                            }
                            Key::Down => {
                                if !entries.is_empty() {
                                    st.palette_selected =
                                        (st.palette_selected + 1).min(entries.len() - 1);
                                }
                            }
                            Key::Enter | Key::KPEnter => {
                                let idx = st.palette_selected.min(entries.len().saturating_sub(1));
                                if let Some(entry) = entries.get(idx) {
                                    let act = entry.action;
                                    st.execute_palette_action(act, &mut tabs, &mut sb);
                                }
                            }
                            Key::BackSpace => {
                                st.palette_query.pop();
                                st.palette_selected = 0;
                            }
                            _ => {
                                let txt = app::event_text();
                                if let Some(ch) = txt.chars().next() {
                                    if !ch.is_control() && !ctrl {
                                        st.palette_query.push(ch);
                                        st.palette_selected = 0;
                                    }
                                }
                            }
                        }
                        drop(st);
                        Self::update_status(&state.borrow(), &mut sb);
                        wid.redraw();
                        return true;
                    }

                    // 1b. Handle Go to Line Overlay if active
                    if state.borrow().overlay == OverlayMode::GoToLine {
                        let mut st = state.borrow_mut();
                        match key {
                            Key::Escape => st.overlay = OverlayMode::None,
                            Key::Enter | Key::KPEnter => {
                                if let Ok(line_num) = st.goto_query.trim().parse::<usize>() {
                                    st.doc_mut().go_to_line(line_num);
                                    st.doc_mut().ensure_cursor_visible(vis_rows, vis_cols);
                                }
                                st.overlay = OverlayMode::None;
                            }
                            Key::BackSpace => {
                                st.goto_query.pop();
                            }
                            _ => {
                                let txt = app::event_text();
                                if let Some(ch) = txt.chars().next() {
                                    if ch.is_ascii_digit() {
                                        st.goto_query.push(ch);
                                    }
                                }
                            }
                        }
                        drop(st);
                        Self::update_status(&state.borrow(), &mut sb);
                        wid.redraw();
                        return true;
                    }

                    // 2. Handle Find & Replace Overlay Keyboard Input if active
                    let current_overlay = state.borrow().overlay;
                    if let OverlayMode::Find {
                        replace,
                        focus_replace,
                    } = current_overlay
                    {
                        let mut st = state.borrow_mut();
                        match key {
                            Key::Escape => {
                                st.overlay = OverlayMode::None;
                            }
                            Key::Tab => {
                                if replace {
                                    st.overlay = OverlayMode::Find {
                                        replace: true,
                                        focus_replace: !focus_replace,
                                    };
                                }
                            }
                            Key::Enter | Key::KPEnter => {
                                let q = st.find_query.clone();
                                let r = st.replace_query.clone();
                                if replace && ctrl {
                                    st.doc_mut().replace_all(&q, &r);
                                } else if replace && focus_replace {
                                    st.doc_mut().replace_current_or_next(&q, &r);
                                } else if shift {
                                    st.doc_mut().find_prev(&q);
                                } else {
                                    st.doc_mut().find_next(&q);
                                }
                                st.doc_mut().ensure_cursor_visible(vis_rows, vis_cols);
                            }
                            Key::BackSpace => {
                                if focus_replace {
                                    st.replace_query.pop();
                                } else {
                                    st.find_query.pop();
                                    let q = st.find_query.clone();
                                    st.doc_mut().find_next(&q);
                                }
                            }
                            _ => {
                                let txt = app::event_text();
                                if let Some(ch) = txt.chars().next() {
                                    if !ch.is_control() && !ctrl {
                                        if focus_replace {
                                            st.replace_query.push(ch);
                                        } else {
                                            st.find_query.push(ch);
                                            let q = st.find_query.clone();
                                            st.doc_mut().find_next(&q);
                                            st.doc_mut().ensure_cursor_visible(vis_rows, vis_cols);
                                        }
                                    }
                                }
                            }
                        }
                        drop(st);
                        Self::update_status(&state.borrow(), &mut sb);
                        wid.redraw();
                        return true;
                    }

                    // 2b. Autocomplete Popup navigation (`Tab` accepts, `Esc` closes)
                    if state.borrow().autocomplete_open {
                        let candidates = state.borrow().doc().completion_candidates();
                        if !candidates.is_empty() {
                            if key == Key::Tab {
                                let mut st = state.borrow_mut();
                                let sel = st.autocomplete_selected.min(candidates.len() - 1);
                                let chosen = candidates[sel].clone();
                                st.doc_mut().apply_completion(&chosen);
                                st.autocomplete_open = false;
                                drop(st);
                                Self::update_status(&state.borrow(), &mut sb);
                                tabs.redraw();
                                wid.redraw();
                                return true;
                            } else if key == Key::Escape {
                                state.borrow_mut().autocomplete_open = false;
                                wid.redraw();
                                return true;
                            }
                        } else {
                            state.borrow_mut().autocomplete_open = false;
                        }
                    }

                    // 3. Optional Vim Modal Engine
                    let vim = state.borrow().vim_mode;
                    if matches!(vim, VimMode::Normal | VimMode::Visual | VimMode::VisualLine)
                        && !ctrl
                        && !alt
                    {
                        let handled =
                            Self::handle_vim_key(&mut state.borrow_mut(), key, vis_rows, vis_cols);
                        if handled {
                            Self::update_status(&state.borrow(), &mut sb);
                            tabs.redraw();
                            wid.redraw();
                            return true;
                        }
                    }

                    if ctrl && !shift && !alt && matches!(key.bits() as u8 as char, 'c' | 'C') {
                        let copied = {
                            let st = state.borrow();
                            let doc = st.doc();
                            doc.selected_text().or_else(|| {
                                doc.lines.get(doc.cursor.line).map(|l| format!("{}\n", l))
                            })
                        };
                        if let Some(txt) = copied {
                            app::copy(&txt);
                        }
                        return true;
                    }
                    if ctrl && !shift && !alt && matches!(key.bits() as u8 as char, 'x' | 'X') {
                        let cut_txt = {
                            let mut st = state.borrow_mut();
                            let doc = st.doc_mut();
                            let txt = doc.selected_text();
                            if txt.is_some() {
                                doc.push_undo(EditKind::BlockOp);
                                doc.delete_selection_internal();
                            }
                            txt
                        };
                        if let Some(txt) = cut_txt {
                            app::copy(&txt);
                        }
                        Self::update_status(&state.borrow(), &mut sb);
                        tabs.redraw();
                        wid.redraw();
                        return true;
                    }
                    if ctrl && !shift && !alt && matches!(key.bits() as u8 as char, 'v' | 'V') {
                        app::paste_text(wid);
                        return true;
                    }
                    if key == Key::F1 {
                        show_about_developer();
                        return true;
                    }

                    let handled = {
                        let mut st = state.borrow_mut();

                        if ctrl && alt && key == Key::Up {
                            st.doc_mut().add_cursor_vertical(-1);
                            true
                        } else if ctrl && alt && key == Key::Down {
                            st.doc_mut().add_cursor_vertical(1);
                            true
                        } else if alt && !ctrl && key == Key::Up {
                            if shift {
                                st.doc_mut().duplicate_lines();
                            } else {
                                st.doc_mut().move_lines_up();
                            }
                            true
                        } else if alt && !ctrl && key == Key::Down {
                            if shift {
                                st.doc_mut().duplicate_lines();
                            } else {
                                st.doc_mut().move_lines_down();
                            }
                            true
                        } else if ctrl {
                            let ch = key.bits() as u8 as char;
                            match ch {
                                'w' | 'W' => {
                                    st.close_active_tab();
                                    tabs.redraw();
                                    true
                                }
                                'd' | 'D' => {
                                    if shift {
                                        st.doc_mut().duplicate_lines();
                                    } else {
                                        st.doc_mut().add_cursor_next_occurrence();
                                    }
                                    true
                                }
                                'i' | 'I' if shift => {
                                    st.doc_mut().format_document();
                                    true
                                }
                                '/' => {
                                    let lang = st.syntax.detect_language_name(
                                        st.doc().path.as_deref(),
                                        &st.doc().title,
                                    );
                                    let prefix = SyntaxEngine::comment_prefix(&lang);
                                    st.doc_mut().toggle_comment(prefix);
                                    true
                                }
                                'a' | 'A' => {
                                    st.doc_mut().select_all();
                                    true
                                }
                                'z' | 'Z' => {
                                    if shift {
                                        st.doc_mut().redo();
                                    } else {
                                        st.doc_mut().undo();
                                    }
                                    true
                                }
                                'y' | 'Y' => {
                                    st.doc_mut().redo();
                                    true
                                }
                                _ => false,
                            }
                        } else {
                            match key {
                                Key::Escape => {
                                    if st.vim_mode == VimMode::Insert {
                                        st.vim_mode = VimMode::Normal;
                                    }
                                    st.autocomplete_open = false;
                                    let doc = st.doc_mut();
                                    doc.clear_extra_cursors();
                                    doc.anchor = None;
                                    true
                                }
                                Key::Left => {
                                    st.autocomplete_open = false;
                                    st.doc_mut().move_left(shift);
                                    true
                                }
                                Key::Right => {
                                    st.autocomplete_open = false;
                                    st.doc_mut().move_right(shift);
                                    true
                                }
                                Key::Up => {
                                    st.autocomplete_open = false;
                                    st.doc_mut().move_up(shift);
                                    true
                                }
                                Key::Down => {
                                    st.autocomplete_open = false;
                                    st.doc_mut().move_down(shift);
                                    true
                                }
                                Key::Home => {
                                    st.autocomplete_open = false;
                                    st.doc_mut().move_home(shift);
                                    true
                                }
                                Key::End => {
                                    st.autocomplete_open = false;
                                    st.doc_mut().move_end(shift);
                                    true
                                }
                                Key::PageUp => {
                                    st.autocomplete_open = false;
                                    let doc = st.doc_mut();
                                    for _ in 0..vis_rows {
                                        doc.move_up(shift);
                                    }
                                    true
                                }
                                Key::PageDown => {
                                    st.autocomplete_open = false;
                                    let doc = st.doc_mut();
                                    for _ in 0..vis_rows {
                                        doc.move_down(shift);
                                    }
                                    true
                                }
                                Key::BackSpace => {
                                    st.doc_mut().backspace();
                                    st.autocomplete_open =
                                        !st.doc().completion_candidates().is_empty();
                                    true
                                }
                                Key::Delete => {
                                    st.doc_mut().delete_forward();
                                    true
                                }
                                Key::Enter | Key::KPEnter => {
                                    st.autocomplete_open = false;
                                    st.doc_mut().insert_newline_auto_indent();
                                    true
                                }
                                Key::Tab => {
                                    st.doc_mut().insert_tab();
                                    true
                                }
                                _ => {
                                    let txt = app::event_text();
                                    if let Some(ch) = txt.chars().next() {
                                        if !ch.is_control() {
                                            st.doc_mut().insert_char(ch);
                                            st.autocomplete_selected = 0;
                                            st.autocomplete_open =
                                                (ch.is_alphanumeric() || ch == '_')
                                                    && !st
                                                        .doc()
                                                        .completion_candidates()
                                                        .is_empty();
                                            true
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    }
                                }
                            }
                        }
                    };

                    if handled {
                        {
                            let mut st = state.borrow_mut();
                            st.doc_mut().ensure_cursor_visible(vis_rows, vis_cols);
                        }
                        Self::update_status(&state.borrow(), &mut sb);
                        tabs.redraw();
                        wid.redraw();
                    }
                    handled
                }
                Event::Paste => {
                    let txt = app::event_text();
                    if !txt.is_empty() {
                        let mut st = state.borrow_mut();
                        st.doc_mut().insert_str(&txt);
                    }
                    Self::update_status(&state.borrow(), &mut sb);
                    tabs.redraw();
                    wid.redraw();
                    true
                }
                _ => false,
            }
        });

        Self { inner }
    }

    fn show_context_menu(
        wid: &mut Widget,
        state: &Rc<RefCell<EditorState>>,
        tabs: &mut Widget,
        sb: &mut Frame,
        mx: i32,
        my: i32,
    ) {
        let items = &[
            "Cut\t",
            "Copy\t",
            "Paste\t",
            "Select All\t",
            "Format Code\t",
            "Toggle Line Comment\t",
            "Duplicate Line\t",
            "Toggle Bookmark\t",
            "Go to Line...\t",
            "Find / Replace...\t",
            "Command Palette...\t",
            "Close File\t",
            "About Developer\t",
        ];
        let popup = MenuItem::new(items);
        if let Some(choice) = popup.popup(mx, my) {
            if let Some(label) = choice.label() {
                match label.trim() {
                    "Cut" => {
                        let cut_txt = {
                            let mut st = state.borrow_mut();
                            let doc = st.doc_mut();
                            let txt = doc.selected_text();
                            if txt.is_some() {
                                doc.push_undo(EditKind::BlockOp);
                                doc.delete_selection_internal();
                            }
                            txt
                        };
                        if let Some(txt) = cut_txt {
                            app::copy(&txt);
                        }
                    }
                    "Copy" => {
                        let copied = state.borrow().doc().selected_text();
                        if let Some(txt) = copied {
                            app::copy(&txt);
                        }
                    }
                    "Paste" => {
                        app::paste_text(wid);
                    }
                    "Select All" => {
                        state.borrow_mut().doc_mut().select_all();
                    }
                    "Format Code" => {
                        state.borrow_mut().doc_mut().format_document();
                    }
                    "Toggle Line Comment" => {
                        let mut st = state.borrow_mut();
                        let lang = st
                            .syntax
                            .detect_language_name(st.doc().path.as_deref(), &st.doc().title);
                        let prefix = SyntaxEngine::comment_prefix(&lang);
                        st.doc_mut().toggle_comment(prefix);
                    }
                    "Duplicate Line" => {
                        state.borrow_mut().doc_mut().duplicate_lines();
                    }
                    "Toggle Bookmark" => {
                        state.borrow_mut().doc_mut().toggle_bookmark();
                    }
                    "Go to Line..." => {
                        let mut st = state.borrow_mut();
                        st.goto_query.clear();
                        st.overlay = OverlayMode::GoToLine;
                    }
                    "Find / Replace..." => {
                        state.borrow_mut().overlay = OverlayMode::Find {
                            replace: true,
                            focus_replace: false,
                        };
                    }
                    "Command Palette..." => {
                        let mut st = state.borrow_mut();
                        st.overlay = OverlayMode::CommandPalette;
                        st.palette_query.clear();
                        st.palette_selected = 0;
                    }
                    "Close File" => {
                        state.borrow_mut().close_active_tab();
                    }
                    "About Developer" => {
                        show_about_developer();
                    }
                    _ => {}
                }
                Self::update_status(&state.borrow(), sb);
                tabs.redraw();
                wid.redraw();
            }
        }
    }

    fn handle_vim_key(
        st: &mut EditorState,
        key: Key,
        vis_rows: usize,
        vis_cols: usize,
    ) -> bool {
        if key == Key::Escape {
            st.vim_mode = VimMode::Normal;
            st.vim_pending_key = None;
            st.doc_mut().anchor = None;
            st.doc_mut().clear_extra_cursors();
            return true;
        }

        let is_visual = matches!(st.vim_mode, VimMode::Visual | VimMode::VisualLine);
        let txt = app::event_text();
        let Some(ch) = txt.chars().next() else {
            return false;
        };

        if let Some(pending) = st.vim_pending_key.take() {
            match (pending, ch) {
                ('g', 'g') => {
                    st.doc_mut().cursor = Pos::new(0, 0);
                    st.doc_mut().ensure_cursor_visible(vis_rows, vis_cols);
                    return true;
                }
                ('d', 'd') => {
                    let removed = st.doc_mut().delete_current_line();
                    st.vim_yank_buffer = format!("{}\n", removed);
                    app::copy(&st.vim_yank_buffer);
                    return true;
                }
                ('y', 'y') => {
                    let l = st.doc().cursor.line;
                    let line_txt = st.doc().lines[l].clone();
                    st.vim_yank_buffer = format!("{}\n", line_txt);
                    app::copy(&st.vim_yank_buffer);
                    return true;
                }
                _ => {}
            }
        }

        match ch {
            'h' => st.doc_mut().move_left(is_visual),
            'l' => st.doc_mut().move_right(is_visual),
            'k' => st.doc_mut().move_up(is_visual),
            'j' => st.doc_mut().move_down(is_visual),
            'w' => st.doc_mut().move_word_forward(is_visual),
            'b' => st.doc_mut().move_word_backward(is_visual),
            '0' => st.doc_mut().move_home(is_visual),
            '$' => st.doc_mut().move_end(is_visual),
            'G' => {
                let last = st.doc().lines.len().saturating_sub(1);
                st.doc_mut().cursor = Pos::new(last, 0);
            }
            'g' | 'd' | 'y' if !is_visual => {
                st.vim_pending_key = Some(ch);
            }
            'i' => {
                st.vim_mode = VimMode::Insert;
                st.doc_mut().anchor = None;
            }
            'a' => {
                st.doc_mut().move_right(false);
                st.vim_mode = VimMode::Insert;
            }
            'I' => {
                st.doc_mut().move_home(false);
                st.vim_mode = VimMode::Insert;
            }
            'A' => {
                st.doc_mut().move_end(false);
                st.vim_mode = VimMode::Insert;
            }
            'o' => {
                st.doc_mut().move_end(false);
                st.doc_mut().insert_newline_auto_indent();
                st.vim_mode = VimMode::Insert;
            }
            'O' => {
                st.doc_mut().move_home(false);
                st.doc_mut().insert_newline_auto_indent();
                st.doc_mut().move_up(false);
                st.vim_mode = VimMode::Insert;
            }
            'v' => {
                st.vim_mode = VimMode::Visual;
                let c = st.doc().cursor;
                st.doc_mut().anchor = Some(c);
            }
            'V' => {
                st.vim_mode = VimMode::VisualLine;
                let l = st.doc().cursor.line;
                let len = st.doc().line_char_len(l);
                st.doc_mut().anchor = Some(Pos::new(l, 0));
                st.doc_mut().cursor = Pos::new(l, len);
            }
            'd' | 'x' if is_visual => {
                if let Some(sel) = st.doc().selected_text() {
                    st.vim_yank_buffer = sel.clone();
                    app::copy(&sel);
                }
                st.doc_mut().push_undo(EditKind::BlockOp);
                st.doc_mut().delete_selection_internal();
                st.vim_mode = VimMode::Normal;
            }
            'y' if is_visual => {
                if let Some(sel) = st.doc().selected_text() {
                    st.vim_yank_buffer = sel.clone();
                    app::copy(&sel);
                }
                st.doc_mut().anchor = None;
                st.vim_mode = VimMode::Normal;
            }
            'x' => {
                st.doc_mut().delete_forward();
            }
            'p' => {
                let yank = st.vim_yank_buffer.clone();
                if !yank.is_empty() {
                    st.doc_mut().insert_str(&yank);
                }
            }
            'u' => {
                st.doc_mut().undo();
            }
            _ => return false,
        }

        if st.vim_mode == VimMode::VisualLine {
            if let Some(a) = st.doc().anchor {
                let c = st.doc().cursor;
                if c.line >= a.line {
                    let end_len = st.doc().line_char_len(c.line);
                    st.doc_mut().anchor = Some(Pos::new(a.line, 0));
                    st.doc_mut().cursor.col = end_len;
                }
            }
        }

        st.doc_mut().ensure_cursor_visible(vis_rows, vis_cols);
        true
    }

    fn mouse_to_pos(wid: &Widget, st: &EditorState, mx: i32, my: i32) -> Pos {
        let doc = st.doc();
        let line_h = st.line_height();
        draw::set_font(Font::Courier, st.font_size);
        let char_w = draw::width("M").max(7.0) as i32;
        let text_x = wid.x() + st.sidebar_w() + GUTTER_WIDTH + 10;

        let vis_lines = doc.visible_doc_lines();
        let scroll_idx = doc.scroll_y.min(vis_lines.len().saturating_sub(1));
        let top_line = vis_lines.get(scroll_idx).copied().unwrap_or(0);
        let sticky_rows = if st.sticky_scroll {
            doc.sticky_headers_for(top_line).len()
        } else {
            0
        };

        let body_y = wid.y() + (sticky_rows as i32) * line_h;
        let rel_y = (my - body_y).max(0);
        let clicked_row = (rel_y / line_h) as usize;
        let vis_idx = (scroll_idx + clicked_row).min(vis_lines.len().saturating_sub(1));
        let line = vis_lines.get(vis_idx).copied().unwrap_or(0);

        let rel_x = (mx - text_x + char_w / 2).max(0);
        let col = doc.scroll_x + (rel_x / char_w) as usize;
        doc.clamp_pos(Pos::new(line, col))
    }

    pub fn update_status(st: &EditorState, sb: &mut Frame) {
        let doc = st.doc();
        let lang = st
            .syntax
            .detect_language_name(doc.path.as_deref(), &doc.title);
        let dirty_mark = if doc.dirty { "● " } else { "" };
        let multi_info = if !doc.extra_cursors.is_empty() {
            format!(" [{} cursors]", doc.extra_cursors.len() + 1)
        } else if let Some(sel) = doc.selected_text() {
            format!(" ({} selected)", sel.chars().count())
        } else {
            String::new()
        };
        let vim_badge = st.vim_mode.badge();
        let zoom_pct = ((st.font_size as f32 / 15.0) * 100.0).round() as i32;

        sb.set_label(&format!(
            " {}{}{}  │  Ln {}, Col {}{}  │  {}  │  {}  │  Zoom: {}%  │  Theme: {}  │  UTF-8",
            vim_badge,
            dirty_mark,
            doc.title,
            doc.cursor.line + 1,
            doc.cursor.col + 1,
            multi_info,
            doc.indent_style.label(),
            lang,
            zoom_pct,
            st.theme().name
        ));
    }
}
