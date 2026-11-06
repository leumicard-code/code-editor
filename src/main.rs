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
use std::path::Path;
use std::rc::Rc;

use fltk::{
    app,
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

use crate::canvas::{CodeCanvas, EditorState};
use crate::document::Document;

const TAB_BAR_HEIGHT: i32 = 34;
const MENU_HEIGHT: i32 = 28;
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

fn main() {
    let app = app::App::default();
    load_studio_fonts();

    let widget_theme = WidgetTheme::new(ThemeType::Dark);
    widget_theme.apply();

    let state = Rc::new(RefCell::new(EditorState::new()));
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

    // Central editor area (Tab Bar + High-Performance CodeCanvas)
    let tab_bar = Widget::new(0, MENU_HEIGHT, win_w, TAB_BAR_HEIGHT, None);

    let editor_y = MENU_HEIGHT + TAB_BAR_HEIGHT;
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
        tab_bar,
        status_bar,
    );

    win.end();
    win.resizable(&canvas_wid);
    win.make_resizable(true);
    win.show();

    let _ = canvas_wid.take_focus();

    app.run().unwrap();
}
