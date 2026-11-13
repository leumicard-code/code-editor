// ============================================================================
// syntax.rs — Multi-Language Syntax Highlighting & Bracket Colorization
// ============================================================================

use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::themes::{EditorTheme, Rgb};

#[derive(Clone, Debug)]
pub struct StyledSpan {
    pub text: String,
    pub fg: Rgb,
}

pub struct SyntaxEngine {
    pub ps: SyntaxSet,
    pub ts: ThemeSet,
}

impl SyntaxEngine {
    pub fn new() -> Self {
        Self {
            ps: SyntaxSet::load_defaults_newlines(),
            ts: ThemeSet::load_defaults(),
        }
    }

    pub fn detect_language_name(&self, path: Option<&Path>, title: &str) -> String {
        let ext = path
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .or_else(|| title.rsplit_once('.').map(|(_, e)| e))
            .unwrap_or("")
            .to_lowercase();
        match ext.as_str() {
            "toml" => "TOML".to_string(),
            "tsx" => "TypeScript React".to_string(),
            "ts" => "TypeScript".to_string(),
            _ => self.find_syntax(path, title).name.clone(),
        }
    }

    #[allow(dead_code)]
    pub fn comment_prefix(lang_name: &str) -> &'static str {
        let lower = lang_name.to_lowercase();
        if lower.contains("python")
            || lower.contains("shell")
            || lower.contains("bash")
            || lower.contains("yaml")
            || lower.contains("toml")
            || lower.contains("ruby")
            || lower.contains("makefile")
        {
            "#"
        } else if lower.contains("sql") || lower.contains("lua") || lower.contains("haskell") {
            "--"
        } else {
            "//"
        }
    }

    fn find_syntax(&self, path: Option<&Path>, title: &str) -> &SyntaxReference {
        if let Some(p) = path {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if let Some(s) = self.ps.find_syntax_by_extension(ext) {
                    return s;
                }
            }
        }
        if let Some((_, ext)) = title.rsplit_once('.') {
            if let Some(s) = self.ps.find_syntax_by_extension(ext) {
                return s;
            }
        }
        self.ps
            .find_syntax_by_extension("rs")
            .unwrap_or_else(|| self.ps.find_syntax_plain_text())
    }

    /// Highlights lines in the visible range (with warm-up context for multi-line constructs).
    pub fn highlight_lines(
        &self,
        lines: &[String],
        start_line: usize,
        end_line: usize,
        path: Option<&Path>,
        title: &str,
        theme: &EditorTheme,
    ) -> Vec<Vec<StyledSpan>> {
        let syntax = self.find_syntax(path, title);
        let syn_theme = self
            .ts
            .themes
            .get(theme.syntect_theme)
            .or_else(|| self.ts.themes.values().next())
            .expect("syntect default themes");

        let mut h = HighlightLines::new(syntax, syn_theme);
        let warmup_start = start_line.saturating_sub(40);
        for line in lines.iter().take(start_line).skip(warmup_start) {
            let _ = h.highlight_line(line, &self.ps);
        }

        let mut out = Vec::with_capacity(end_line.saturating_sub(start_line));
        for line in lines.iter().take(end_line).skip(start_line) {
            match h.highlight_line(line, &self.ps) {
                Ok(ranges) => {
                    let spans = ranges
                        .into_iter()
                        .map(|(style, s): (Style, &str)| StyledSpan {
                            text: s.to_string(),
                            fg: Rgb(style.foreground.r, style.foreground.g, style.foreground.b),
                        })
                        .collect();
                    out.push(spans);
                }
                Err(_) => {
                    out.push(vec![StyledSpan {
                        text: line.clone(),
                        fg: theme.text_fg,
                    }]);
                }
            }
        }
        out
    }
}
