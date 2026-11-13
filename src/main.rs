// ============================================================================
// main.rs — Studio Code Editor IDE (FLTK + Syntect + Custom Fonts + Persistence)
// ============================================================================

mod canvas;
mod config;
mod document;
mod menu;
mod syntax;
mod themes;

use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;

use fltk::{
    app,
    dialog::{NativeFileChooser, NativeFileChooserType},
    draw,
    enums::{Align, Color, Cursor, Event, Font, FrameType},
    frame::Frame,
    group::Group,
    menu::SysMenuBar,
    prelude::*,
    widget::Widget,
    window::Window,
};
use fltk_theme::{ThemeType, WidgetTheme};

use crate::canvas::{CodeCanvas, EditorState, OverlayMode, show_font_picker_dialog};
use crate::document::Document;
use crate::themes::EditorTheme;

const MENU_HEIGHT: i32 = 28;
const TOOLBAR_HEIGHT: i32 = 38;
const TAB_BAR_HEIGHT: i32 = 34;
const STATUS_HEIGHT: i32 = 28;
const TAB_WIDTH: i32 = 160;

/// Loads bundled Cascadia Code & Figtree fonts from `assets/fonts/` or `~/.fonts/`.
fn load_studio_fonts() {
    let code_candidates = [
        "assets/fonts/CascadiaCode-Regular.ttf",
        "/home/shay/.fonts/CascadiaCode-Regular.ttf",
    ];
    for p in code_candidates {
        if Path::new(p).exists() {
            if let Ok(name) = Font::load_font(p) {
                Font::set_font(Font::Courier, &name);
                break;
            }
        }
    }

    let code_bold_candidates = [
        "assets/fonts/CascadiaCode-Bold.ttf",
        "/home/shay/.fonts/CascadiaCode-Bold.ttf",
    ];
    for p in code_bold_candidates {
        if Path::new(p).exists() {
            if let Ok(name) = Font::load_font(p) {
                Font::set_font(Font::CourierBold, &name);
                break;
            }
        }
    }

    let ui_candidates = [
        "assets/fonts/Figtree-SemiBold.ttf",
        "/home/shay/.fonts/Figtree-SemiBold.ttf",
    ];
    for p in ui_candidates {
        if Path::new(p).exists() {
            if let Ok(name) = Font::load_font(p) {
                Font::set_font(Font::Helvetica, &name);
                Font::set_font(Font::HelveticaBold, &name);
                break;
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolbarAction {
    NewFile,
    OpenFile,
    SaveFile,
    OpenFolder,
    Undo,
    Redo,
    Find,
    Replace,
    FontPicker,
    ZoomOut,
    ZoomReset,
    ZoomIn,
    ToggleSidebar,
    ToggleMinimap,
    CycleTheme,
}

struct ToolbarBtn {
    x: i32,
    w: i32,
    action: ToolbarAction,
}

fn toolbar_buttons() -> [ToolbarBtn; 15] {
    [
        ToolbarBtn { x: 6, w: 66, action: ToolbarAction::NewFile },
        ToolbarBtn { x: 74, w: 70, action: ToolbarAction::OpenFile },
        ToolbarBtn { x: 146, w: 70, action: ToolbarAction::SaveFile },
        ToolbarBtn { x: 218, w: 80, action: ToolbarAction::OpenFolder },
        ToolbarBtn { x: 308, w: 66, action: ToolbarAction::Undo },
        ToolbarBtn { x: 376, w: 66, action: ToolbarAction::Redo },
        ToolbarBtn { x: 452, w: 66, action: ToolbarAction::Find },
        ToolbarBtn { x: 520, w: 80, action: ToolbarAction::Replace },
        ToolbarBtn { x: 610, w: 185, action: ToolbarAction::FontPicker },
        ToolbarBtn { x: 799, w: 34, action: ToolbarAction::ZoomOut },
        ToolbarBtn { x: 835, w: 52, action: ToolbarAction::ZoomReset },
        ToolbarBtn { x: 889, w: 34, action: ToolbarAction::ZoomIn },
        ToolbarBtn { x: 933, w: 84, action: ToolbarAction::ToggleSidebar },
        ToolbarBtn { x: 1019, w: 70, action: ToolbarAction::ToggleMinimap },
        ToolbarBtn { x: 1091, w: 76, action: ToolbarAction::CycleTheme },
    ]
}

fn wire_toolbar(
    mut toolbar: Widget,
    state: Rc<RefCell<EditorState>>,
    mut tab_bar: Widget,
    mut canvas_wid: Widget,
    mut status_bar: Frame,
) {
    let hover_idx: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));

    toolbar.draw({
        let state = Rc::clone(&state);
        let hover_idx = Rc::clone(&hover_idx);
        move |wid| {
            let st = state.borrow();
            let theme = st.theme();
            let wx = wid.x();
            let wy = wid.y();
            let ww = wid.w();
            let wh = wid.h();
            let hov = *hover_idx.borrow();

            draw::push_clip(wx, wy, ww, wh);

            // Toolbar background & subtle bottom separator line (Kate-style)
            draw::set_draw_color(theme.gutter_bg.to_fltk());
            draw::draw_rectf(wx, wy, ww, wh);
            draw::set_draw_color(theme.indent_guide.to_fltk());
            draw::draw_line(wx, wy + wh - 1, wx + ww, wy + wh - 1);

            // Vertical dividers between groups
            for sep_x in [302, 446, 604, 927] {
                draw::set_draw_color(theme.indent_guide.to_fltk());
                draw::draw_line(wx + sep_x, wy + 7, wx + sep_x, wy + wh - 8);
            }

            let btns = toolbar_buttons();
            for (idx, b) in btns.iter().enumerate() {
                let bx = wx + b.x;
                let by = wy + 4;
                let bw = b.w;
                let bh = wh - 9;
                if bx + bw > wx + ww {
                    continue;
                }

                let is_hovered = hov == Some(idx);
                let is_toggled = matches!(
                    b.action,
                    ToolbarAction::ToggleSidebar if st.show_sidebar
                ) || matches!(
                    b.action,
                    ToolbarAction::ToggleMinimap if st.show_minimap
                );

                if is_hovered || is_toggled || b.action == ToolbarAction::FontPicker {
                    let bg = if is_hovered {
                        theme.selection_bg.to_fltk()
                    } else if is_toggled {
                        theme.active_line_bg.to_fltk()
                    } else {
                        theme.bg.to_fltk()
                    };
                    draw::set_draw_color(bg);
                    draw::draw_rectf(bx, by, bw, bh);
                    draw::set_draw_color(if is_hovered || is_toggled {
                        theme.accent.to_fltk()
                    } else {
                        theme.indent_guide.to_fltk()
                    });
                    draw::draw_rect(bx, by, bw, bh);
                }

                let icon_col = theme.accent.to_fltk();
                let txt_col = theme.text_fg.to_fltk();
                let ix = bx + 8;
                let iy = by + 6;

                match b.action {
                    ToolbarAction::NewFile => {
                        // Document outline with '+' badge
                        draw::set_draw_color(icon_col);
                        draw::draw_rect(ix, iy, 12, 15);
                        draw::draw_line(ix + 3, iy + 7, ix + 9, iy + 7);
                        draw::draw_line(ix + 6, iy + 4, ix + 6, iy + 10);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("New", ix + 16, by, bw - 22, bh, Align::Left | Align::Inside);
                    }
                    ToolbarAction::OpenFile => {
                        // Folder outline icon
                        draw::set_draw_color(icon_col);
                        draw::draw_rect(ix, iy + 3, 14, 11);
                        draw::draw_line(ix, iy + 1, ix + 6, iy + 1);
                        draw::draw_line(ix + 6, iy + 1, ix + 8, iy + 3);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("Open", ix + 18, by, bw - 24, bh, Align::Left | Align::Inside);
                    }
                    ToolbarAction::SaveFile => {
                        // Floppy / Save icon
                        draw::set_draw_color(icon_col);
                        draw::draw_rect(ix, iy + 1, 13, 13);
                        draw::draw_rect(ix + 3, iy + 1, 7, 5);
                        draw::draw_rect(ix + 2, iy + 8, 9, 6);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        let lbl = if st.doc().dirty { "Save ●" } else { "Save" };
                        draw::draw_text2(lbl, ix + 17, by, bw - 22, bh, Align::Left | Align::Inside);
                    }
                    ToolbarAction::OpenFolder => {
                        draw::set_draw_color(icon_col);
                        draw::draw_rect(ix, iy + 3, 14, 11);
                        draw::draw_rectf(ix + 2, iy + 6, 10, 6);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("Folder", ix + 18, by, bw - 24, bh, Align::Left | Align::Inside);
                    }
                    ToolbarAction::Undo => {
                        draw::set_font(Font::HelveticaBold, 14);
                        draw::set_draw_color(icon_col);
                        draw::draw_text2("↶", ix - 2, by, 18, bh, Align::Center | Align::Inside);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("Undo", ix + 16, by, bw - 22, bh, Align::Left | Align::Inside);
                    }
                    ToolbarAction::Redo => {
                        draw::set_font(Font::HelveticaBold, 14);
                        draw::set_draw_color(icon_col);
                        draw::draw_text2("↷", ix - 2, by, 18, bh, Align::Center | Align::Inside);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("Redo", ix + 16, by, bw - 22, bh, Align::Left | Align::Inside);
                    }
                    ToolbarAction::Find => {
                        draw::set_draw_color(icon_col);
                        draw::draw_circle((ix + 5) as f64, (iy + 6) as f64, 4.5);
                        draw::draw_line(ix + 9, iy + 10, ix + 13, iy + 14);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("Find", ix + 17, by, bw - 22, bh, Align::Left | Align::Inside);
                    }
                    ToolbarAction::Replace => {
                        draw::set_font(Font::HelveticaBold, 13);
                        draw::set_draw_color(icon_col);
                        draw::draw_text2("⇄", ix - 2, by, 18, bh, Align::Center | Align::Inside);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("Replace", ix + 16, by, bw - 22, bh, Align::Left | Align::Inside);
                    }
                    ToolbarAction::FontPicker => {
                        draw::set_font(Font::CourierBold, 12);
                        draw::set_draw_color(icon_col);
                        draw::draw_text2("Aa", bx + 6, by, 24, bh, Align::Center | Align::Inside);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        let mut fname = st.font_name.clone();
                        if fname.chars().count() > 16 {
                            fname = format!("{}…", fname.chars().take(15).collect::<String>());
                        }
                        draw::draw_text2(
                            &format!("{} ▾", fname),
                            bx + 32,
                            by,
                            bw - 36,
                            bh,
                            Align::Left | Align::Inside,
                        );
                    }
                    ToolbarAction::ZoomOut => {
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("A-", bx, by, bw, bh, Align::Center | Align::Inside);
                    }
                    ToolbarAction::ZoomReset => {
                        draw::set_font(Font::CourierBold, 12);
                        draw::set_draw_color(icon_col);
                        draw::draw_text2(
                            &format!("{}px", st.font_size),
                            bx,
                            by,
                            bw,
                            bh,
                            Align::Center | Align::Inside,
                        );
                    }
                    ToolbarAction::ZoomIn => {
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("A+", bx, by, bw, bh, Align::Center | Align::Inside);
                    }
                    ToolbarAction::ToggleSidebar => {
                        draw::set_draw_color(icon_col);
                        draw::draw_rect(ix, iy + 1, 14, 13);
                        draw::draw_line(ix + 5, iy + 1, ix + 5, iy + 13);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("Sidebar", ix + 18, by, bw - 22, bh, Align::Left | Align::Inside);
                    }
                    ToolbarAction::ToggleMinimap => {
                        draw::set_draw_color(icon_col);
                        draw::draw_rect(ix, iy + 1, 13, 13);
                        draw::draw_line(ix + 9, iy + 1, ix + 9, iy + 13);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::set_draw_color(txt_col);
                        draw::draw_text2("Map", ix + 17, by, bw - 20, bh, Align::Left | Align::Inside);
                    }
                    ToolbarAction::CycleTheme => {
                        draw::set_draw_color(icon_col);
                        draw::draw_rectf(ix, iy + 3, 11, 11);
                        draw::set_draw_color( txt_col);
                        draw::draw_rect(ix, iy + 3, 11, 11);
                        draw::set_font(Font::HelveticaBold, 12);
                        draw::draw_text2("Theme", ix + 15, by, bw - 18, bh, Align::Left | Align::Inside);
                    }
                }
            }

            draw::pop_clip();
        }
    });

    toolbar.handle(move |wid, ev| match ev {
        Event::Enter | Event::Move => {
            let mx = app::event_x() - wid.x();
            let btns = toolbar_buttons();
            let found = btns
                .iter()
                .position(|b| mx >= b.x && mx <= b.x + b.w);
            if *hover_idx.borrow() != found {
                *hover_idx.borrow_mut() = found;
                wid.redraw();
            }
            if let Some(mut w) = wid.window() {
                w.set_cursor(if found.is_some() {
                    Cursor::Hand
                } else {
                    Cursor::Default
                });
            }
            true
        }
        Event::Leave => {
            *hover_idx.borrow_mut() = None;
            if let Some(mut w) = wid.window() {
                w.set_cursor(Cursor::Default);
            }
            wid.redraw();
            true
        }
        Event::Push => {
            let mx = app::event_x() - wid.x();
            let btns = toolbar_buttons();
            if let Some(b) = btns.iter().find(|b| mx >= b.x && mx <= b.x + b.w) {
                match b.action {
                    ToolbarAction::NewFile => {
                        let mut st = state.borrow_mut();
                        let next = st.docs.len() + 1;
                        let title = format!("untitled_{}.rs", next);
                        st.docs.push(Document::new_with_text(&title, None, ""));
                        st.active_doc = st.docs.len() - 1;
                        st.save_session();
                    }
                    ToolbarAction::OpenFile => {
                        let mut chooser =
                            NativeFileChooser::new(NativeFileChooserType::BrowseFile);
                        chooser.show();
                        let path = chooser.filename();
                        if !path.as_os_str().is_empty() {
                            state.borrow_mut().open_file_from_path(path);
                        }
                    }
                    ToolbarAction::SaveFile => {
                        let existing_path = state.borrow().doc().path.clone();
                        let target_path = if let Some(p) = existing_path {
                            Some(p)
                        } else {
                            let mut chooser =
                                NativeFileChooser::new(NativeFileChooserType::BrowseSaveFile);
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
                            if std::fs::write(&path, content).is_ok() {
                                let mut st = state.borrow_mut();
                                let doc = st.doc_mut();
                                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                                    doc.title = name.to_string();
                                }
                                doc.path = Some(path);
                                doc.dirty = false;
                                st.save_session();
                            }
                        }
                    }
                    ToolbarAction::OpenFolder => {
                        let mut chooser =
                            NativeFileChooser::new(NativeFileChooserType::BrowseDir);
                        chooser.show();
                        let dir = chooser.filename();
                        if !dir.as_os_str().is_empty() && dir.is_dir() {
                            let mut st = state.borrow_mut();
                            st.workspace_dir = dir;
                            st.show_sidebar = true;
                            st.sidebar_scroll = 0;
                            st.refresh_sidebar();
                            st.save_session();
                        }
                    }
                    ToolbarAction::Undo => {
                        state.borrow_mut().doc_mut().undo();
                    }
                    ToolbarAction::Redo => {
                        state.borrow_mut().doc_mut().redo();
                    }
                    ToolbarAction::Find => {
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
                    }
                    ToolbarAction::Replace => {
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
                    }
                    ToolbarAction::FontPicker => {
                        show_font_picker_dialog(
                            Rc::clone(&state),
                            canvas_wid.clone(),
                            Some(wid.clone()),
                            status_bar.clone(),
                        );
                    }
                    ToolbarAction::ZoomOut => {
                        state.borrow_mut().zoom_out();
                    }
                    ToolbarAction::ZoomReset => {
                        state.borrow_mut().zoom_reset();
                    }
                    ToolbarAction::ZoomIn => {
                        state.borrow_mut().zoom_in();
                    }
                    ToolbarAction::ToggleSidebar => {
                        let mut st = state.borrow_mut();
                        st.show_sidebar = !st.show_sidebar;
                        st.save_session();
                    }
                    ToolbarAction::ToggleMinimap => {
                        let mut st = state.borrow_mut();
                        st.show_minimap = !st.show_minimap;
                        st.save_session();
                    }
                    ToolbarAction::CycleTheme => {
                        let mut st = state.borrow_mut();
                        st.theme_idx = (st.theme_idx + 1) % EditorTheme::all().len();
                        let th = st.theme();
                        status_bar.set_color(th.status_bg.to_fltk());
                        status_bar.set_label_color(th.status_fg.to_fltk());
                        status_bar.redraw();
                        st.save_session();
                    }
                }
                CodeCanvas::update_status(&state.borrow(), &mut status_bar);
                wid.redraw();
                tab_bar.redraw();
                canvas_wid.redraw();
                let _ = canvas_wid.take_focus();
            }
            true
        }
        _ => false,
    });
}

fn wire_tab_bar(
    mut tab_bar: Widget,
    state: Rc<RefCell<EditorState>>,
    mut canvas_wid: Widget,
    mut status_bar: Frame,
) {
    tab_bar.draw({
        let state = Rc::clone(&state);
        move |wid| {
            let st = state.borrow();
            let theme = st.theme();
            let wx = wid.x();
            let wy = wid.y();
            let ww = wid.w();
            let wh = wid.h();

            draw::push_clip(wx, wy, ww, wh);
            draw::set_draw_color(theme.tab_bar_bg.to_fltk());
            draw::draw_rectf(wx, wy, ww, wh);

            draw::set_font(Font::HelveticaBold, 13);

            for (i, doc) in st.docs.iter().enumerate() {
                let tx = wx + (i as i32) * TAB_WIDTH;
                if tx >= wx + ww - 40 {
                    break;
                }
                let is_active = i == st.active_doc;
                let bg = if is_active { theme.bg } else { theme.tab_bar_bg };
                draw::set_draw_color(bg.to_fltk());
                draw::draw_rectf(tx, wy, TAB_WIDTH - 2, wh);

                if is_active {
                    draw::set_draw_color(theme.accent.to_fltk());
                    draw::draw_rectf(tx, wy, TAB_WIDTH - 2, 3);
                }

                let fg = if is_active {
                    theme.text_fg
                } else {
                    theme.gutter_fg
                };
                draw::set_draw_color(fg.to_fltk());
                let dirty_dot = if doc.dirty { " ●" } else { "" };
                let label = format!("{}{}", doc.title, dirty_dot);
                draw::draw_text2(
                    &label,
                    tx + 12,
                    wy + 2,
                    TAB_WIDTH - 36,
                    wh - 2,
                    Align::Left | Align::Inside,
                );

                // Close '×' button on tab
                draw::set_draw_color(theme.gutter_fg.to_fltk());
                draw::draw_text2(
                    "×",
                    tx + TAB_WIDTH - 26,
                    wy + 2,
                    20,
                    wh - 2,
                    Align::Center | Align::Inside,
                );
            }

            // '+' button to open a new editor tab
            let plus_x = wx + (st.docs.len() as i32) * TAB_WIDTH + 4;
            if plus_x + 30 < wx + ww {
                draw::set_draw_color(theme.accent.to_fltk());
                draw::draw_text2("+", plus_x, wy, 28, wh, Align::Center | Align::Inside);
            }

            draw::pop_clip();
        }
    });

    tab_bar.handle(move |wid, ev| match ev {
        Event::Enter => {
            if let Some(mut w) = wid.window() {
                w.set_cursor(Cursor::Hand);
            }
            true
        }
        Event::Leave => {
            if let Some(mut w) = wid.window() {
                w.set_cursor(Cursor::Default);
            }
            true
        }
        Event::Push => {
            let mx = app::event_x() - wid.x();
            let mut st = state.borrow_mut();
            let count = st.docs.len();
            let plus_x = (count as i32) * TAB_WIDTH + 4;

            if mx >= plus_x && mx <= plus_x + 30 {
                let next = count + 1;
                let title = format!("untitled_{}.rs", next);
                st.docs.push(Document::new_with_text(&title, None, ""));
                st.active_doc = st.docs.len() - 1;
                st.save_session();
            } else {
                let idx = (mx / TAB_WIDTH) as usize;
                if idx < count {
                    let in_tab_x = mx % TAB_WIDTH;
                    if in_tab_x >= TAB_WIDTH - 28 {
                        if count > 1 {
                            st.docs.remove(idx);
                            if st.active_doc >= st.docs.len() {
                                st.active_doc = st.docs.len() - 1;
                            }
                        } else {
                            st.docs[0] = Document::new_with_text("untitled_1.rs", None, "");
                        }
                    } else {
                        st.active_doc = idx;
                    }
                    st.save_session();
                }
            }
            drop(st);
            CodeCanvas::update_status(&state.borrow(), &mut status_bar);
            wid.redraw();
            canvas_wid.redraw();
            let _ = canvas_wid.take_focus();
            true
        }
        _ => false,
    });
}

#[cfg(unix)]
fn ipc_socket_path() -> PathBuf {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    std::env::temp_dir().join(format!("studio_code_editor_{}.sock", user))
}

fn main() {
    let cli_args: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty() && !p.to_string_lossy().starts_with('-'))
        .map(|p| crate::canvas::resolve_cli_path(&p))
        .collect();

    // If an instance of Studio Code Editor is already running and files were passed via
    // Context Menu ("Open With") or CLI, forward the files to the existing instance and exit.
    #[cfg(unix)]
    let sock_path = ipc_socket_path();

    #[cfg(unix)]
    if !cli_args.is_empty() {
        if let Ok(mut stream) = UnixStream::connect(&sock_path) {
            for p in &cli_args {
                let _ = writeln!(stream, "{}", p.to_string_lossy());
            }
            let _ = stream.flush();
            return;
        }
    }

    let app = app::App::default();
    load_studio_fonts();

    let widget_theme = WidgetTheme::new(ThemeType::Dark);
    widget_theme.apply();

    let state = Rc::new(RefCell::new(EditorState::new_with_args(&cli_args)));
    let init_theme = state.borrow().theme();

    let win_w = 1180;
    let win_h = 760;

    let mut win = Window::default()
        .with_size(win_w, win_h)
        .with_label("Studio Code Editor — FLTK + Syntect IDE");

    let mut menu_bar = SysMenuBar::default().with_size(win_w, MENU_HEIGHT).with_pos(0, 0);
    menu_bar.set_frame(FrameType::FlatBox);
    menu_bar.set_color(init_theme.tab_bar_bg.to_fltk());
    menu_bar.set_text_color(init_theme.text_fg.to_fltk());

    // Bottom status bar
    let bottom_y = win_h - STATUS_HEIGHT;
    let bottom_group = Group::default().with_size(win_w, STATUS_HEIGHT).with_pos(0, bottom_y);
    let mut accent_line = Frame::default().with_size(win_w, 2).with_pos(0, bottom_y);
    accent_line.set_frame(FrameType::FlatBox);
    accent_line.set_color(init_theme.accent.to_fltk());

    let mut status_bar = Frame::default()
        .with_size(win_w, STATUS_HEIGHT - 2)
        .with_pos(0, bottom_y + 2);
    status_bar.set_frame(FrameType::FlatBox);
    status_bar.set_color(init_theme.status_bg.to_fltk());
    status_bar.set_label_color(Color::from_rgb(
        init_theme.status_fg.0,
        init_theme.status_fg.1,
        init_theme.status_fg.2,
    ));
    status_bar.set_label_size(13);
    status_bar.set_align(Align::Left | Align::Inside);
    bottom_group.end();

    // Central editor area (Kate-style Toolbar + Tab Bar + High-Performance CodeCanvas)
    let toolbar = Widget::new(0, MENU_HEIGHT, win_w, TOOLBAR_HEIGHT, None);
    let tab_bar = Widget::new(0, MENU_HEIGHT + TOOLBAR_HEIGHT, win_w, TAB_BAR_HEIGHT, None);

    let editor_y = MENU_HEIGHT + TOOLBAR_HEIGHT + TAB_BAR_HEIGHT;
    let editor_h = win_h - editor_y - STATUS_HEIGHT;

    let canvas = CodeCanvas::new(
        0,
        editor_y,
        win_w,
        editor_h,
        Rc::clone(&state),
        tab_bar.clone(),
        status_bar.clone(),
    );
    let mut canvas_wid = canvas.as_base_widget();

    wire_toolbar(
        toolbar.clone(),
        Rc::clone(&state),
        tab_bar.clone(),
        canvas_wid.clone(),
        status_bar.clone(),
    );

    wire_tab_bar(
        tab_bar.clone(),
        Rc::clone(&state),
        canvas_wid.clone(),
        status_bar.clone(),
    );

    CodeCanvas::update_status(&state.borrow(), &mut status_bar);

    menu::setup(
        &mut menu_bar,
        Rc::clone(&state),
        canvas_wid.clone(),
        tab_bar.clone(),
        status_bar.clone(),
    );

    win.end();
    win.resizable(&canvas_wid);
    win.make_resizable(true);
    win.show();

    let _ = canvas_wid.take_focus();

    // Background Unix Socket Listener so right-click "Open With" opens a new tab in this window
    #[cfg(unix)]
    let (ipc_tx, ipc_rx) = mpsc::channel::<PathBuf>();

    #[cfg(unix)]
    {
        if UnixStream::connect(&sock_path).is_err() {
            let _ = std::fs::remove_file(&sock_path);
            if let Ok(listener) = UnixListener::bind(&sock_path) {
                std::thread::spawn(move || {
                    for stream in listener.incoming().flatten() {
                        let reader = BufReader::new(stream);
                        for line in reader.lines().map_while(Result::ok) {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                let _ = ipc_tx.send(PathBuf::from(trimmed));
                            }
                        }
                        app::awake();
                    }
                });
            }
        }

        let state_ipc = Rc::clone(&state);
        let mut tab_bar_ipc = tab_bar.clone();
        let mut canvas_ipc = canvas_wid.clone();
        let mut sb_ipc = status_bar.clone();
        let mut win_ipc = win.clone();
        app::add_idle3(move |_| {
            let mut opened = false;
            while let Ok(path) = ipc_rx.try_recv() {
                state_ipc.borrow_mut().open_file_from_path(path);
                opened = true;
            }
            if opened {
                CodeCanvas::update_status(&state_ipc.borrow(), &mut sb_ipc);
                tab_bar_ipc.redraw();
                canvas_ipc.redraw();
                win_ipc.show();
                let _ = canvas_ipc.take_focus();
            }
        });
    }

    app.run().unwrap();

    #[cfg(unix)]
    let _ = std::fs::remove_file(&sock_path);
}
