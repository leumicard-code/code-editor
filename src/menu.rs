// ============================================================================
// menu.rs — English Studio MenuBar (File, Edit, View, Themes, Help/About)
// ============================================================================

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use fltk::{
    app,
    dialog::{NativeFileChooser, NativeFileChooserType},
    enums::{Key, Shortcut},
    frame::Frame,
    menu::{MenuFlag, SysMenuBar},
    prelude::*,
    widget::Widget,
};

use crate::canvas::{show_about_developer, CodeCanvas, EditorState, OverlayMode};
use crate::document::{Document, IndentStyle};
use crate::syntax::SyntaxEngine;
use crate::themes::EditorTheme;

pub fn setup(
    menu: &mut SysMenuBar,
    state: Rc<RefCell<EditorState>>,
    canvas_wid: Widget,
    tab_bar: Widget,
    status_bar: Frame,
) {
    // ── File -> New Tab (Ctrl+N) ─────────────────────────────────────────────
    menu.add(
        "&File/&New Tab\t",
        Shortcut::Ctrl | 'n',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut tabs = tab_bar.clone();
            let mut sb = status_bar.clone();
            move |_| {
                {
                    let mut st = state.borrow_mut();
                    let next_num = st.docs.len() + 1;
                    let title = format!("untitled_{}.rs", next_num);
                    st.docs.push(Document::new_with_text(&title, None, ""));
                    st.active_doc = st.docs.len() - 1;
                }
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                tabs.redraw();
                canvas.redraw();
            }
        },
    );

    // ── File -> Open File... (Ctrl+O) ────────────────────────────────────────
    menu.add(
        "&File/&Open File...\t",
        Shortcut::Ctrl | 'o',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut tabs = tab_bar.clone();
            let mut sb = status_bar.clone();
            move |_| {
                let mut chooser = NativeFileChooser::new(NativeFileChooserType::BrowseFile);
                chooser.set_filter("Code & Text Files\t*.{rs,toml,py,js,ts,c,cpp,h,json,md,txt,sh,yml,yaml}\nAll Files\t*.*");
                chooser.show();

                let path = chooser.filename();
                if !path.as_os_str().is_empty() {
                    state.borrow_mut().open_file_from_path(path);
                    CodeCanvas::update_status(&state.borrow(), &mut sb);
                    tabs.redraw();
                    canvas.redraw();
                }
            }
        },
    );

    // ── File -> Open Folder... (Ctrl+Shift+O) ────────────────────────────────
    menu.add(
        "&File/Open &Folder in Sidebar...\t",
        Shortcut::Ctrl | Shortcut::Shift | 'o',
        MenuFlag::MenuDivider,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut chooser = NativeFileChooser::new(NativeFileChooserType::BrowseDir);
                chooser.show();
                let dir = chooser.filename();
                if !dir.as_os_str().is_empty() && dir.is_dir() {
                    let mut st = state.borrow_mut();
                    st.workspace_dir = dir;
                    st.show_sidebar = true;
                    st.sidebar_scroll = 0;
                    st.refresh_sidebar();
                    st.save_session();
                    drop(st);
                    canvas.redraw();
                }
            }
        },
    );

    // ── File -> Save (Ctrl+S) ────────────────────────────────────────────────
    menu.add(
        "&File/&Save\t",
        Shortcut::Ctrl | 's',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut tabs = tab_bar.clone();
            let mut sb = status_bar.clone();
            move |_| {
                let existing_path = state.borrow().doc().path.clone();
                let target_path = if let Some(p) = existing_path {
                    Some(p)
                } else {
                    let mut chooser = NativeFileChooser::new(NativeFileChooserType::BrowseSaveFile);
                    chooser.show();
                    let p = chooser.filename();
                    if p.as_os_str().is_empty() {
                        None
                    } else {
                        Some(p)
                    }
                };

                if let Some(path) = target_path {
                    let content = state.borrow().doc().text();
                    match fs::write(&path, content) {
                        Ok(_) => {
                            let mut st = state.borrow_mut();
                            let doc = st.doc_mut();
                            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                                doc.title = name.to_string();
                            }
                            doc.path = Some(path);
                            doc.dirty = false;
                            st.save_session();
                        }
                        Err(e) => {
                            fltk::dialog::alert_default(&format!("Failed to save file:\n{}", e));
                        }
                    }
                    CodeCanvas::update_status(&state.borrow(), &mut sb);
                    tabs.redraw();
                }
            }
        },
    );

    // ── File -> Save As... (Ctrl+Shift+S) ────────────────────────────────────
    menu.add(
        "&File/Save &As...\t",
        Shortcut::Ctrl | Shortcut::Shift | 's',
        MenuFlag::MenuDivider,
        {
            let state = Rc::clone(&state);
            let mut tabs = tab_bar.clone();
            let mut sb = status_bar.clone();
            move |_| {
                let mut chooser = NativeFileChooser::new(NativeFileChooserType::BrowseSaveFile);
                chooser.show();
                let path = chooser.filename();
                if !path.as_os_str().is_empty() {
                    let content = state.borrow().doc().text();
                    match fs::write(&path, content) {
                        Ok(_) => {
                            let mut st = state.borrow_mut();
                            let doc = st.doc_mut();
                            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                                doc.title = name.to_string();
                            }
                            doc.path = Some(path);
                            doc.dirty = false;
                            st.save_session();
                        }
                        Err(e) => {
                            fltk::dialog::alert_default(&format!("Failed to save file:\n{}", e));
                        }
                    }
                    CodeCanvas::update_status(&state.borrow(), &mut sb);
                    tabs.redraw();
                }
            }
        },
    );

    // ── File -> Format Code (Ctrl+Shift+I) ───────────────────────────────────
    menu.add(
        "&File/&Format Code\t",
        Shortcut::Ctrl | Shortcut::Shift | 'i',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut tabs = tab_bar.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().format_document();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                tabs.redraw();
                canvas.redraw();
            }
        },
    );

    // ── File -> Close File (Ctrl+W) ──────────────────────────────────────────
    menu.add(
        "&File/&Close File\t",
        Shortcut::Ctrl | 'w',
        MenuFlag::MenuDivider,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut tabs = tab_bar.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().close_active_tab();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                tabs.redraw();
                canvas.redraw();
            }
        },
    );

    // ── File -> Exit (Ctrl+Q) ────────────────────────────────────────────────
    menu.add(
        "&File/E&xit\t",
        Shortcut::Ctrl | 'q',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            move |_| {
                state.borrow().save_session();
                app::quit();
            }
        },
    );

    // ── Edit Menu ────────────────────────────────────────────────────────────
    menu.add(
        "&Edit/&Undo\t",
        Shortcut::Ctrl | 'z',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().undo();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/&Redo\t",
        Shortcut::Ctrl | 'y',
        MenuFlag::MenuDivider,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().redo();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/Cu&t\t",
        Shortcut::Ctrl | 'x',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                let cut_txt = {
                    let mut st = state.borrow_mut();
                    let doc = st.doc_mut();
                    let txt = doc.selected_text();
                    if txt.is_some() {
                        doc.push_undo(crate::document::EditKind::BlockOp);
                        doc.delete_selection_internal();
                    }
                    txt
                };
                if let Some(txt) = cut_txt {
                    app::copy(&txt);
                }
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/&Copy\t",
        Shortcut::Ctrl | 'c',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            move |_| {
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
            }
        },
    );

    menu.add(
        "&Edit/&Paste\t",
        Shortcut::Ctrl | 'v',
        MenuFlag::MenuDivider,
        {
            let canvas = canvas_wid.clone();
            move |_| {
                app::paste_text(&canvas);
            }
        },
    );

    menu.add(
        "&Edit/&Find...\t",
        Shortcut::Ctrl | 'f',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                state.borrow_mut().overlay = OverlayMode::Find {
                    replace: false,
                    focus_replace: false,
                };
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/&Replace...\t",
        Shortcut::Ctrl | 'h',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                state.borrow_mut().overlay = OverlayMode::Find {
                    replace: true,
                    focus_replace: false,
                };
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/&Go to Line...\t",
        Shortcut::Ctrl | 'g',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                st.goto_query.clear();
                st.overlay = OverlayMode::GoToLine;
                drop(st);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/Toggle &Bookmark on Line\t",
        Shortcut::Ctrl | Shortcut::from_key(Key::F2),
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                state.borrow_mut().doc_mut().toggle_bookmark();
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/Jump to Next Bookmark\t",
        Shortcut::from_key(Key::F2),
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().next_bookmark();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/Command &Palette...\t",
        Shortcut::Ctrl | Shortcut::Shift | 'p',
        MenuFlag::MenuDivider,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                st.overlay = OverlayMode::CommandPalette;
                st.palette_query.clear();
                st.palette_selected = 0;
                drop(st);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/Format &Document\t",
        Shortcut::Shift | Shortcut::Alt | 'f',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut tabs = tab_bar.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().format_document();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                tabs.redraw();
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/Toggle Line &Comment\t",
        Shortcut::Ctrl | '/',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut tabs = tab_bar.clone();
            let mut sb = status_bar.clone();
            move |_| {
                {
                    let mut st = state.borrow_mut();
                    let lang = st
                        .syntax
                        .detect_language_name(st.doc().path.as_deref(), &st.doc().title);
                    let prefix = SyntaxEngine::comment_prefix(&lang);
                    st.doc_mut().toggle_comment(prefix);
                }
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                tabs.redraw();
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/&Duplicate Line / Selection\t",
        Shortcut::Ctrl | Shortcut::Shift | 'd',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().duplicate_lines();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/Add Cursor to Next Occurrence\t",
        Shortcut::Ctrl | 'd',
        MenuFlag::MenuDivider,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().add_cursor_next_occurrence();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&Edit/Select &All\t",
        Shortcut::Ctrl | 'a',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().select_all();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    // ── View Menu ────────────────────────────────────────────────────────────
    menu.add(
        "&View/File &Explorer Sidebar\t",
        Shortcut::Ctrl | 'b',
        MenuFlag::Toggle | MenuFlag::Value,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                st.show_sidebar = !st.show_sidebar;
                st.save_session();
                drop(st);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/Code &Minimap\t",
        Shortcut::Ctrl | 'm',
        MenuFlag::Toggle | MenuFlag::Value,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                st.show_minimap = !st.show_minimap;
                st.save_session();
                drop(st);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/Zoom &In Font\t",
        Shortcut::Ctrl | '=',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().zoom_in();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/Zoom &Out Font\t",
        Shortcut::Ctrl | '-',
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().zoom_out();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/&Reset Zoom (100%)\t",
        Shortcut::Ctrl | '0',
        MenuFlag::MenuDivider,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().zoom_reset();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/&Vim Mode (Normal / Insert / Visual)\t",
        Shortcut::Ctrl | Shortcut::Alt | 'v',
        MenuFlag::Toggle,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().toggle_vim_mode();
                CodeCanvas::update_status(&state.borrow(), &mut sb);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/&Word Wrap\t",
        Shortcut::Alt | 'z',
        MenuFlag::Toggle,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                let doc = st.doc_mut();
                doc.wrap_lines = !doc.wrap_lines;
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/&Sticky Scroll Headers\t",
        Shortcut::None,
        MenuFlag::Toggle | MenuFlag::Value,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                st.sticky_scroll = !st.sticky_scroll;
                st.save_session();
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/Inline &Color Previews (#rrggbb)\t",
        Shortcut::None,
        MenuFlag::Toggle | MenuFlag::Value,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                st.inline_color_previews = !st.inline_color_previews;
                st.save_session();
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/&Rainbow Brackets\t",
        Shortcut::None,
        MenuFlag::Toggle | MenuFlag::Value,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                st.rainbow_brackets = !st.rainbow_brackets;
                st.save_session();
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/Render &Whitespace (· / →)\t",
        Shortcut::None,
        MenuFlag::Toggle,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                st.show_whitespace = !st.show_whitespace;
                st.save_session();
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/&Indentation Guides\t",
        Shortcut::None,
        MenuFlag::Toggle | MenuFlag::Value | MenuFlag::MenuDivider,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                st.show_indent_guides = !st.show_indent_guides;
                st.save_session();
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/&Toggle Fold at Cursor\t",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                let mut st = state.borrow_mut();
                let line = st.doc().cursor.line;
                st.doc_mut().toggle_fold_at(line);
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/&Unfold All Blocks\t",
        Shortcut::None,
        MenuFlag::MenuDivider,
        {
            let state = Rc::clone(&state);
            let mut canvas = canvas_wid.clone();
            move |_| {
                state.borrow_mut().doc_mut().unfold_all();
                canvas.redraw();
            }
        },
    );

    menu.add(
        "&View/Indent: 4 Spaces\t",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().indent_style = IndentStyle::Spaces(4);
                CodeCanvas::update_status(&state.borrow(), &mut sb);
            }
        },
    );

    menu.add(
        "&View/Indent: 2 Spaces\t",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().indent_style = IndentStyle::Spaces(2);
                CodeCanvas::update_status(&state.borrow(), &mut sb);
            }
        },
    );

    menu.add(
        "&View/Indent: Tabs\t",
        Shortcut::None,
        MenuFlag::Normal,
        {
            let state = Rc::clone(&state);
            let mut sb = status_bar.clone();
            move |_| {
                state.borrow_mut().doc_mut().indent_style = IndentStyle::Tabs;
                CodeCanvas::update_status(&state.borrow(), &mut sb);
            }
        },
    );

    // ── Themes Submenu (24 Built-in Studio & Iced Themes) ────────────────────
    for (idx, theme) in EditorTheme::all().iter().enumerate() {
        let label = format!("&Themes/{}", theme.name);
        menu.add(
            &label,
            Shortcut::None,
            MenuFlag::Normal,
            {
                let state = Rc::clone(&state);
                let mut canvas = canvas_wid.clone();
                let mut tabs = tab_bar.clone();
                let mut sb = status_bar.clone();
                move |m| {
                    {
                        let mut st = state.borrow_mut();
                        st.theme_idx = idx;
                        let th = st.theme();
                        m.set_color(th.tab_bar_bg.to_fltk());
                        m.set_text_color(th.text_fg.to_fltk());
                        sb.set_color(th.status_bg.to_fltk());
                        sb.set_label_color(th.status_fg.to_fltk());
                        st.save_session();
                    }
                    CodeCanvas::update_status(&state.borrow(), &mut sb);
                    m.redraw();
                    tabs.redraw();
                    canvas.redraw();
                    sb.redraw();
                }
            },
        );
    }

    // ── Help -> About Developer ──────────────────────────────────────────────
    menu.add(
        "&Help/&About Developer (Shay Kadosh)\t",
        Shortcut::from_key(Key::F1),
        MenuFlag::Normal,
        |_| {
            show_about_developer();
        },
    );
}
