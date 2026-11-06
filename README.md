Studio Code Editor — FLTK + Syntect IDE

    High-Performance Native IDE Engine • 3.6 MiB • Million Dollar UI

Lead Architect & Developer: Shay Kadosh • 2000-2026 • All Rights Reserved

Rust
FLTK
Size
RAM
License
No Tank

VSCode Killer in 3.6 MiB!

VSCode = 1.5GB Electron tank. KATE = 50 MiB. Studio Code Editor = 3.6 MiB FLTK 1.5 + Syntect!
✨ Engine Highlights

From About Dialog:

    Custom 2D Hardware-Friendly Canvas Renderer (FLTK 1.5) - Fast, no battery, no tank!
    Multi-Language Syntax Highlighting via Syntect - Rust, TOML, MD, etc.
    24 Built-in Studio & Iced Themes - Solarized Light, Catppuccin Mocha, Matrix Emerald, Dark Elegance, Frost Light
    Code Folding, Sticky Scroll, Rainbow Brackets & Inline Color Swatches
    Multi-Cursor Editing, Command Palette (Ctrl+Shift+P) & Optional Vim Mode
    File Explorer: ICED12 - Tree with Parent Directory, assets, src, Cargo.toml
    MINIMAP - Right side mini map with blue viewport, hardware-friendly!
    Zoom: 107% - Like VSCode! For glasses! Ctrl+ / Ctrl- / Ctrl0
    Multi-Tab - untitled_1.rs, Cargo.toml, + button
    Status Bar - File | Ln, Col | Spaces: 4 | Rust | Zoom | Theme | UTF-8
    Hebrew Native - No font loading! System Noto works!

📸 Screenshots
Explorer + Minimap + Solarized Light

    Explorer ICED12 left, editor center with Cargo.toml, minimap right
    Syntax: [package] orange, name = "my_audio_player" green
    Status: Cargo.toml | Ln 1, Col 1 | Spaces: 4 | Rust | Zoom: 107% | Theme: Solarized Light | UTF-8

Studio Code Editor — FLTK + Syntect IDE

    Tabs: main.rs, config.toml, untitled_3.rs
    Catppuccin Mocha theme
    Status: main.rs | Ln 25, Col 41 | Spaces: 4 | Rust | Theme: Catppuccin Mocha | UTF-8

🏗️ Architecture - 7 Files • 3.6 MiB

text/
├── src/
│   ├── main.rs       # App, Window 900x600, entry
│   ├── canvas.rs     # CodeCanvas, EditorState, Rc<RefCell>, Minimap, Zoom 107%, 30 FPS tick
│   ├── document.rs   # Document, TextBuffer, File IO
│   ├── menu.rs       # SysMenuBar File Edit View Themes Help
│   ├── syntax.rs     # Syntect Engine highlighting
│   ├── themes.rs     # 24 Themes Solarized, Catppuccin, Matrix
│   └── config.rs     # Config, Zoom, Spaces, UTF-8
└── Cargo.toml        # [profile.release] lto=true codegen-units=1

Like syseq architecture:

    daemon.rs 51.5 MiB + UI 8.3 MiB + engine + biquad Q=1.41 + spectrum 2048 Hann
    This: 7 files + 3.6 MiB + FLTK 1.5 + Syntect!

🚀 Build - Release Optimized

cargo build --release
# Finished release profile [optimized] target(s) in 38.12s
# Location: /home/shay/code1/text/target/release
# Size: 3.6 MiB (3,806,640) - Executable
./text

Cargo.toml:

[profile.release]
lto = true
codegen-units = 1

Dependencies:

fltk = "1.5.23"
fltk-theme = "0.7.9"
syntect = "..."

💡 Philosophy - Simple Hardware + Smart Software = Million Dollar

Like our other projects:

    Audio: 20₪ AliExpress earphones + Sub-Bass Boost +12dB Biquad RBJ Q=1.41 = Sounds like 700₪!
    Mouse: Wired 20₪ 1ms polling vs Wireless 200₪ 20ms + battery = Wired wins!
    IDE: FLTK 3.6 MiB vs VSCode 1.5GB Electron tank = FLTK million dollar!

No Tank, No Battery, No Guessing!

    Hebrew native - No fonts.rs with include_bytes! Segoe UI proprietary!
    System fonts via Fontconfig fallback - Mandriva Noto works!
    Everything here, no guessing!

👓 For Glasses - Zoom 107%

After 1 hour everything looks the same! With glasses harder!

    20-20-20 Rule: 20 min code, 20 sec window Ashkelon, 20 steps
    Zoom: 107% like VSCode, for glasses!
    Font: Courier 14-16, not small!
    Theme: Solarized Light bright for day, Catppuccin Mocha dark for night - 24 themes!

Linus 56 with Claude Code 23x faster 13.3M→304M because after 1 hour he can't see! Same for us!
🤖 Solo + AI Agents = Team of 10

Lead Architect alone? Can't write everything!

With AI Agents:

    Gemini Argon - Writes Rust with no errors at all!
    Claude Code - Like Linus, 23x faster, finds 13-year-old bugs!
    Antigravity - Agent with editor, terminal, browser - Works on Linux!

2000-2026 Amdocs C++ team of 20 → 2026 Shay Kadosh + AI Agents = team of 20!
📦 Install
Flathub (soon)

Soon on Flathub - 3.6 MiB!

From Release

wget https://github.com/leumicard-code/text/releases/latest/download/text
chmod +x text
./text

From Source

git clone https://github.com/leumicard-code/text
cd text
cargo run --release

🎯 Roadmap

    Stage 1: Core Canvas, Syntect & Multi-Tab Focus - Done! 3.6 MiB!
    File Explorer ICED12 + Minimap + Zoom 107% - Done!
    24 Themes + Vim Mode + Multi-Cursor + Command Palette - Done!
    Stage 2: LSP, Auto-complete, Diagnostics
    Stage 3: Git integration, Terminal panel
    PRO: Per-project profiles 19₪ via Gumroad

📝 License

MIT - Free, open, million dollar!

Built with: Rust 🦀 + FLTK 1.5 + Syntect + Mandriva Linux + KATE + Wired Mouse 20₪ 1ms + Earphones 20₪ + Sub-Bass Boost +12dB

No Tank, No Battery, No Guessing, Million Dollar UI!

Shay Kadosh • System Architect 2000-2026 • Lead Architect & Developer • Ashkelon, Israel
