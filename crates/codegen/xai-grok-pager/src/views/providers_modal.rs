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
    pub models: Option<usize>,
}

/// Which view the Providers modal is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvidersView {
    List,
    Detail {
        provider_idx: usize,
        api_key: String,
        base_url: String,
        focused_field: usize,
        show_api_key: bool,
    },
}

/// State for the Providers modal.
pub struct ProvidersModalState {
    pub window: mw::ModalWindowState,
    selected: usize,
    scroll_offset: usize,
    providers: Vec<ProviderEntry>,
    mode: ProvidersView,
}

const VISIBLE_ROWS: usize = 8;

impl Default for ProvidersModalState {
    fn default() -> Self {
        Self::new(&[])
    }
}

impl ProvidersModalState {
    pub fn new(configured: &[&str]) -> Self {
        Self {
            window: mw::ModalWindowState::new(),
            selected: 0,
            scroll_offset: 0,
            providers: builtin_providers(configured),
            mode: ProvidersView::List,
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

fn builtin_providers(configured: &[&str]) -> Vec<ProviderEntry> {
    fn is_configured(id: &str, configured: &[&str]) -> bool {
        configured.contains(&id)
    }

    vec![
        ProviderEntry {
            id: "xai".into(),
            name: "xAI".into(),
            status: if is_configured("xai", configured) {
                "Connected"
            } else {
                "Not configured"
            },
            status_color: if is_configured("xai", configured) { Color::Green } else { Color::Red },
            endpoint: "api.x.ai".into(),
            configured: is_configured("xai", configured),
            models: None,
        },
        ProviderEntry {
            id: "openai".into(),
            name: "OpenAI".into(),
            status: if is_configured("openai", configured) {
                "Connected"
            } else {
                "Not configured"
            },
            status_color: if is_configured("openai", configured) { Color::Green } else { Color::Red },
            endpoint: "api.openai.com".into(),
            configured: is_configured("openai", configured),
            models: None,
        },
        ProviderEntry {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            status: if is_configured("anthropic", configured) {
                "Connected"
            } else {
                "Not configured"
            },
            status_color: if is_configured("anthropic", configured) {
                Color::Green
            } else {
                Color::Red
            },
            endpoint: "api.anthropic.com".into(),
            configured: is_configured("anthropic", configured),
            models: None,
        },
        ProviderEntry {
            id: "opencode".into(),
            name: "OpenCode Zen".into(),
            status: if is_configured("opencode", configured) {
                "Connected"
            } else {
                "Free tier"
            },
            status_color: if is_configured("opencode", configured) {
                Color::Green
            } else {
                Color::Gray
            },
            endpoint: "opencode.ai".into(),
            configured: is_configured("opencode", configured),
            models: None,
        },
        ProviderEntry {
            id: "ollama".into(),
            name: "Ollama".into(),
            status: "Local",
            status_color: Color::Cyan,
            endpoint: "localhost:11434".into(),
            configured: true,
            models: None,
        },
        ProviderEntry {
            id: "openai-compatible".into(),
            name: "OpenAI Compatible".into(),
            status: "Not configured",
            status_color: Color::Red,
            endpoint: "\u{2014}".into(),
            configured: false,
            models: None,
        },
    ]
}

/// Outcome from handling a key press on the Providers modal.
pub enum ProvidersKeyOutcome {
    Close,
    Changed,
    Unchanged,
}

fn open_detail(state: &mut ProvidersModalState) {
    let Some(provider) = state.selected_provider().cloned() else {
        return;
    };
    state.mode = ProvidersView::Detail {
        provider_idx: state.selected,
        api_key: String::new(),
        base_url: provider.endpoint.clone(),
        focused_field: 0,
        show_api_key: false,
    };
}

/// Handle key events for the Providers modal.
pub fn handle_providers_key(
    state: &mut ProvidersModalState,
    key: &crossterm::event::KeyEvent,
) -> ProvidersKeyOutcome {
    match &state.mode {
        ProvidersView::List => handle_list_key(state, key),
        ProvidersView::Detail { .. } => handle_detail_key(state, key),
    }
}

fn handle_list_key(
    state: &mut ProvidersModalState,
    key: &crossterm::event::KeyEvent,
) -> ProvidersKeyOutcome {
    use crossterm::event::KeyCode;

    match key.code {
        KeyCode::Esc | KeyCode::F(2) => ProvidersKeyOutcome::Close,
        KeyCode::Enter => {
            open_detail(state);
            ProvidersKeyOutcome::Changed
        }
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
        _ => ProvidersKeyOutcome::Unchanged,
    }
}

fn handle_detail_key(
    state: &mut ProvidersModalState,
    key: &crossterm::event::KeyEvent,
) -> ProvidersKeyOutcome {
    use crossterm::event::KeyCode;

    let ProvidersView::Detail {
        ref provider_idx,
        ref mut api_key,
        ref mut base_url,
        ref mut focused_field,
        ref mut show_api_key,
        ..
    } = state.mode
    else {
        return ProvidersKeyOutcome::Changed;
    };

    match key.code {
        KeyCode::Esc => {
            state.mode = ProvidersView::List;
            ProvidersKeyOutcome::Changed
        }
        KeyCode::Tab => {
            *focused_field = (*focused_field + 1) % 3;
            ProvidersKeyOutcome::Changed
        }
        KeyCode::BackTab => {
            *focused_field = if *focused_field == 0 {
                2
            } else {
                *focused_field - 1
            };
            ProvidersKeyOutcome::Changed
        }
        KeyCode::Char(c) if key.modifiers.is_empty() => {
            match *focused_field {
                0 => api_key.push(c),
                1 => base_url.push(c),
                _ => {}
            }
            ProvidersKeyOutcome::Changed
        }
        KeyCode::Backspace => {
            match *focused_field {
                0 => {
                    api_key.pop();
                }
                1 => {
                    base_url.pop();
                }
                _ => {}
            }
            ProvidersKeyOutcome::Changed
        }
        KeyCode::Char('r')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            *show_api_key = !*show_api_key;
            ProvidersKeyOutcome::Changed
        }
        KeyCode::Enter => {
            if let Some(provider) = state.providers.get(*provider_idx) {
                let key = api_key.clone();
                let url = base_url.clone();
                save_provider_config(&provider.id, &key, &url);
            }
            state.mode = ProvidersView::List;
            ProvidersKeyOutcome::Changed
        }
        _ => ProvidersKeyOutcome::Unchanged,
    }
}

fn save_provider_config(id: &str, api_key: &str, base_url: &str) {
    let config_path = xai_grok_config::grok_home().join("config.toml");
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = content.parse().unwrap_or_default();
    let provider = doc
        .entry("provider")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .expect("[provider] must be a table");
    let entry = provider
        .entry(id)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .expect("[provider.<id>] must be a table");
    if !api_key.is_empty() {
        entry["api_key"] = toml_edit::value(api_key);
    }
    if !base_url.is_empty() {
        entry["base_url"] = toml_edit::value(base_url);
    }
    let _ = std::fs::write(&config_path, doc.to_string());
}

fn adjust_scroll(state: &mut ProvidersModalState) {
    if state.selected < state.scroll_offset {
        state.scroll_offset = state.selected;
    } else if state.selected >= state.scroll_offset + VISIBLE_ROWS {
        state.scroll_offset = state.selected + 1 - VISIBLE_ROWS;
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
    match &state.mode {
        ProvidersView::List => render_list(buf, area, state, compact, theme),
        ProvidersView::Detail { .. } => render_detail(buf, area, state, compact, theme),
    }
}

fn render_list(
    buf: &mut Buffer,
    area: Rect,
    state: &mut ProvidersModalState,
    compact: bool,
    theme: &Theme,
) {
    let shortcuts: &[Shortcut<'static>] = &[
        Shortcut {
            label: "\u{2191}/\u{2193} nav",
            clickable: false,
            id: 0,
        },
        Shortcut {
            label: "Enter configure",
            clickable: false,
            id: 1,
        },
        Shortcut {
            label: "Esc close",
            clickable: false,
            id: 2,
        },
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

    let header_style = Style::default().fg(theme.gray).add_modifier(Modifier::BOLD);
    let header = Line::styled(
        format!("{:<14} {:>5} {:>10}  {}", "Provider", "Models", "Status", "Endpoint"),
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

        let bg = if is_selected {
            theme.bg_light
        } else {
            theme.bg_base
        };
        let fg = if is_selected {
            theme.text_primary
        } else {
            theme.gray_bright
        };

        let status_style = Style::default().fg(provider.status_color).bg(bg);
        let row_style = Style::default().fg(fg).bg(bg);

        let models_label = provider.models.map(|n| n.to_string()).unwrap_or_else(|| "—".into());
        let line = Line::from(vec![
            ratatui::text::Span::styled(format!(" {:<14}", provider.name), row_style),
            ratatui::text::Span::styled(format!(" {:>5} ", models_label), row_style),
            ratatui::text::Span::styled(format!(" {:>10} ", provider.status), status_style),
            ratatui::text::Span::styled(format!("  {}", provider.endpoint), row_style),
        ]);
        line.render(Rect::new(row_area.x, y, row_area.width, 1), buf);
    }
}

fn render_detail(
    buf: &mut Buffer,
    area: Rect,
    state: &mut ProvidersModalState,
    compact: bool,
    theme: &Theme,
) {
    let detail = match &state.mode {
        ProvidersView::Detail {
            provider_idx,
            api_key,
            base_url,
            focused_field,
            show_api_key,
        } => (
            *provider_idx,
            api_key.clone(),
            base_url.clone(),
            *focused_field,
            *show_api_key,
        ),
        _ => return,
    };
    let (provider_idx, api_key_str, base_url_str, focused_field, show_api_key) = detail;

    let Some(provider) = state.providers.get(provider_idx) else {
        return;
    };

    let title = format!("Configure: {}", provider.name);

    let shortcuts: &[Shortcut<'static>] = &[
        Shortcut {
            label: "Tab next",
            clickable: false,
            id: 0,
        },
        Shortcut {
            label: "Ctrl+R reveal",
            clickable: false,
            id: 1,
        },
        Shortcut {
            label: "Enter save",
            clickable: false,
            id: 2,
        },
        Shortcut {
            label: "Esc back",
            clickable: false,
            id: 3,
        },
    ];

    let cfg = mw::ModalWindowConfig {
        title: &title,
        tabs: None,
        shortcuts,
        sizing: mw::ModalSizing {
            width_pct: 0.50,
            max_width: 72,
            min_width: 40,
            v_margin: 6,
            h_pad: 3,
            v_pad: 2,
            footer_lines: 2,
        }
        .with_compact(compact),
        fold_info: None,
    };

    let Some(content) = mw::render_modal_window(buf, area, &mut state.window, &cfg, theme) else {
        return;
    };

    let mut render_y = content.content.y;
    let field_label = "API Key:";
    let display_val = if show_api_key {
        api_key_str.clone()
    } else if api_key_str.is_empty() {
        "(not set)".to_string()
    } else {
        "\u{25cf}".repeat(api_key_str.len().min(20))
    };
    render_field(
        buf,
        content.inner_x,
        render_y,
        content.inner_width,
        field_label,
        &display_val,
        focused_field == 0,
        theme,
    );
    render_y += 1;

    let field_label = "Base URL:";
    render_field(
        buf,
        content.inner_x,
        render_y,
        content.inner_width,
        field_label,
        &base_url_str,
        focused_field == 1,
        theme,
    );
    render_y += 1;

    let field_label = "Status:";
    let status_style = Style::default().fg(provider.status_color);
    let status_line = Line::from(vec![
        ratatui::text::Span::styled(
            format!("  {}  ", field_label),
            Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
        ),
        ratatui::text::Span::styled(provider.status, status_style),
    ]);
    status_line.render(
        Rect::new(content.inner_x, render_y, content.inner_width, 1),
        buf,
    );
    render_y += 1;

    let endpoint_line = Line::from(vec![
        ratatui::text::Span::styled(
            "  Endpoint:  ",
            Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
        ),
        ratatui::text::Span::styled(&provider.endpoint, Style::default().fg(theme.gray_bright)),
    ]);
    endpoint_line.render(
        Rect::new(content.inner_x, render_y, content.inner_width, 1),
        buf,
    );
    render_y += 1;

    let hint = Line::styled(
        "Tab to switch fields  \u{2022}  Ctrl+R to toggle API key visibility  \u{2022}  Enter to save",
        Style::default().fg(theme.gray_dim),
    );
    hint.render(
        Rect::new(content.inner_x, render_y + 1, content.inner_width, 1),
        buf,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_field(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    label: &str,
    value: &str,
    focused: bool,
    theme: &Theme,
) {
    let bg = if focused {
        theme.bg_light
    } else {
        theme.bg_base
    };
    let label_style = Style::default()
        .fg(theme.gray)
        .add_modifier(Modifier::BOLD)
        .bg(bg);
    let value_style = Style::default()
        .fg(if focused {
            theme.text_primary
        } else {
            theme.gray_bright
        })
        .bg(bg);

    let line = Line::from(vec![
        ratatui::text::Span::styled(format!("  {}  ", label), label_style),
        ratatui::text::Span::styled(value, value_style),
    ]);
    line.render(Rect::new(x, y, width, 1), buf);
}
