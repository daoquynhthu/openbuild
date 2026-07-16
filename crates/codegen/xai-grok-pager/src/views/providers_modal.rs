use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::theme::Theme;
use crate::views::modal_window::{self as mw, Shortcut};

/// Provider entry for the modal list.
#[derive(Debug, Clone)]
pub struct ProviderEntry {
    pub id: String,
    pub name: String,
    pub status: &'static str,
    pub status_color: Color,
    pub endpoint: String,
    pub configured: bool,
}

/// State for the Providers modal.
pub struct ProvidersModalState {
    pub window: mw::ModalWindowState,
    pub selected: usize,
    pub scroll_offset: usize,
    pub providers: Vec<ProviderEntry>,
}

impl ProvidersModalState {
    pub fn new() -> Self {
        Self {
            window: mw::ModalWindowState::new(),
            selected: 0,
            scroll_offset: 0,
            providers: builtin_providers(),
        }
    }

    fn visible_range(&self, height: usize) -> std::ops::Range<usize> {
        let start = self
            .scroll_offset
            .min(self.providers.len().saturating_sub(1));
        let end = (start + height).min(self.providers.len());
        start..end
    }

    pub fn selected_provider(&self) -> Option<&ProviderEntry> {
        self.providers.get(self.selected)
    }
}

fn builtin_providers() -> Vec<ProviderEntry> {
    vec![
        ProviderEntry {
            id: "xai".into(),
            name: "xAI".into(),
            status: "Connected",
            status_color: Color::Green,
            endpoint: "api.x.ai".into(),
            configured: true,
        },
        ProviderEntry {
            id: "openai".into(),
            name: "OpenAI".into(),
            status: "Not configured",
            status_color: Color::Red,
            endpoint: "api.openai.com".into(),
            configured: false,
        },
        ProviderEntry {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            status: "Not configured",
            status_color: Color::Red,
            endpoint: "api.anthropic.com".into(),
            configured: false,
        },
        ProviderEntry {
            id: "opencode".into(),
            name: "OpenCode Zen".into(),
            status: "Free tier",
            status_color: Color::Gray,
            endpoint: "opencode.ai".into(),
            configured: false,
        },
        ProviderEntry {
            id: "ollama".into(),
            name: "Ollama".into(),
            status: "Local",
            status_color: Color::Cyan,
            endpoint: "localhost:11434".into(),
            configured: true,
        },
        ProviderEntry {
            id: "openai-compatible".into(),
            name: "OpenAI Compatible".into(),
            status: "Not configured",
            status_color: Color::Red,
            endpoint: "\u{2014}".into(),
            configured: false,
        },
    ]
}

/// Outcome from handling a key press on the Providers modal.
pub enum ProvidersKeyOutcome {
    Close,
    Changed,
}

/// Handle key events for the Providers modal.
pub fn handle_providers_key(
    state: &mut ProvidersModalState,
    key: &crossterm::event::KeyEvent,
) -> ProvidersKeyOutcome {
    use crossterm::event::KeyCode;

    match key.code {
        KeyCode::Esc | KeyCode::F(2) => ProvidersKeyOutcome::Close,
        KeyCode::Up | KeyCode::Char('k') if key.modifiers.is_empty() => {
            if state.selected > 0 {
                state.selected -= 1;
                adjust_scroll(state);
            }
            ProvidersKeyOutcome::Changed
        }
        KeyCode::Down | KeyCode::Char('j') if key.modifiers.is_empty() => {
            if state.selected + 1 < state.providers.len() {
                state.selected += 1;
                adjust_scroll(state);
            }
            ProvidersKeyOutcome::Changed
        }
        KeyCode::Home => {
            state.selected = 0;
            state.scroll_offset = 0;
            ProvidersKeyOutcome::Changed
        }
        KeyCode::End => {
            state.selected = state.providers.len().saturating_sub(1);
            ProvidersKeyOutcome::Changed
        }
        _ => ProvidersKeyOutcome::Changed,
    }
}

fn adjust_scroll(state: &mut ProvidersModalState) {
    let visible = 8usize;
    if state.selected < state.scroll_offset {
        state.scroll_offset = state.selected;
    } else if state.selected >= state.scroll_offset + visible {
        state.scroll_offset = state.selected + 1 - visible;
    }
}

/// Render the Providers modal into the given buffer area.
pub fn render_providers_modal(
    buf: &mut Buffer,
    area: Rect,
    state: &mut ProvidersModalState,
    compact: bool,
    theme: &Theme,
) {
    let shortcuts: &[Shortcut<'static>] = &[
        Shortcut { label: "\u{2191}/\u{2193} nav", clickable: false, id: 0 },
        Shortcut { label: "Enter configure", clickable: false, id: 1 },
        Shortcut { label: "Esc close", clickable: false, id: 2 },
    ];

    let cfg = mw::ModalWindowConfig {
        title: "Providers",
        tabs: None,
        shortcuts,
        sizing: mw::ModalSizing {
            width_pct: 0.55,
            max_width: 80,
            min_width: 44,
            v_margin: 4,
            h_pad: 2,
            v_pad: 1,
            footer_lines: 2,
        }
        .with_compact(compact),
        fold_info: None,
    };

    let Some(content) = mw::render_modal_window(buf, area, &mut state.window, &cfg, theme) else {
        return;
    };

    let header_style = Style::default()
        .fg(theme.gray)
        .add_modifier(Modifier::BOLD);
    let header = Line::styled(
        format!("{:<14} {:>16}  {}", "Provider", "Status", "Endpoint"),
        header_style,
    );
    header.render(
        Rect::new(content.inner_x, content.content.y, content.inner_width, 1),
        buf,
    );

    let row_area = Rect::new(
        content.inner_x,
        content.content.y + 1,
        content.inner_width,
        content.content.height.saturating_sub(1),
    );

    let range = state.visible_range(row_area.height as usize);

    for (i, idx) in range.enumerate() {
        let Some(provider) = state.providers.get(idx) else {
            continue;
        };
        let y = row_area.top() + i as u16;
        let is_selected = idx == state.selected;

        let bg = if is_selected { theme.bg_light } else { theme.bg_base };
        let fg = if is_selected { theme.text_primary } else { theme.gray_bright };

        let status_style = Style::default().fg(provider.status_color).bg(bg);
        let row_style = Style::default().fg(fg).bg(bg);

        let line = Line::from(vec![
            ratatui::text::Span::styled(format!(" {:<14}", provider.name), row_style),
            ratatui::text::Span::styled(format!(" {:>16} ", provider.status), status_style),
            ratatui::text::Span::styled(format!("  {}", provider.endpoint), row_style),
        ]);
        line.render(Rect::new(row_area.x, y, row_area.width, 1), buf);
    }
}
