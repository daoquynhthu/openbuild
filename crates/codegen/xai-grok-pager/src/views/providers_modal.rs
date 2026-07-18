use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::config_toml_edit::read_config_document_for_edit;
use crate::provider_state::{CredentialState, ProviderState, ProviderView};
use crate::theme::Theme;
use crate::views::modal_window::{self as mw, Shortcut};

/// Which view the Providers modal is showing.
#[derive(Clone, PartialEq, Eq)]
pub enum ProvidersView {
    List,
    Detail {
        provider_idx: usize,
        env_var_name: String,
        api_key: String,
        base_url: String,
        focused_field: usize,
        show_api_key: bool,
    },
}

impl std::fmt::Debug for ProvidersView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::List => f.debug_struct("List").finish(),
            Self::Detail {
                provider_idx,
                env_var_name,
                api_key: _,
                base_url,
                focused_field,
                show_api_key,
            } => f
                .debug_struct("Detail")
                .field("provider_idx", provider_idx)
                .field("env_var_name", env_var_name)
                .field("api_key", &"[redacted]")
                .field("base_url", base_url)
                .field("focused_field", focused_field)
                .field("show_api_key", show_api_key)
                .finish(),
        }
    }
}

/// State for the Providers modal.
pub struct ProvidersModalState {
    pub window: mw::ModalWindowState,
    selected: usize,
    scroll_offset: usize,
    pub provider_state: ProviderState,
    mode: ProvidersView,
}

const VISIBLE_ROWS: usize = 8;

impl ProvidersModalState {
    pub fn new(provider_state: ProviderState) -> Self {
        Self {
            window: mw::ModalWindowState::new(),
            selected: 0,
            scroll_offset: 0,
            provider_state,
            mode: ProvidersView::List,
        }
    }

    fn visible_range(&self, height: usize) -> std::ops::Range<usize> {
        let providers = self.provider_state.ordered_views();
        let start = self
            .scroll_offset
            .min(providers.len().saturating_sub(1));
        let end = (start + height).min(providers.len());
        start..end
    }

    pub fn selected_provider(&self) -> Option<&ProviderView> {
        let providers = self.provider_state.ordered_views();
        providers.get(self.selected).copied()
    }

    pub fn reset_to_list(&mut self) {
        self.mode = ProvidersView::List;
    }
}

fn credential_status(view: &ProviderView) -> (&'static str, Color) {
    match view.credential {
        CredentialState::NotRequired => ("Not required", Color::Cyan),
        CredentialState::Configured => ("Configured", Color::Green),
        CredentialState::Missing => ("Missing", Color::Red),
        CredentialState::Session => ("Session", Color::Blue),
        CredentialState::Public => ("Free tier", Color::Gray),
    }
}

/// Outcome from handling a key press on the Providers modal.
pub enum ProvidersKeyOutcome {
    Close,
    Save {
        provider_id: String,
        env_var_name: String,
        api_key: String,
        base_url: String,
    },
    Changed,
    Unchanged,
}

fn open_detail(state: &mut ProvidersModalState) {
    let Some(provider) = state.selected_provider().cloned() else {
        return;
    };
    let env_prefill = provider.env_key.first().cloned().unwrap_or_default();
    state.mode = ProvidersView::Detail {
        provider_idx: state.selected,
        env_var_name: env_prefill,
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
            let providers = state.provider_state.ordered_views();
            if state.selected + 1 < providers.len() {
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
            let providers = state.provider_state.ordered_views();
            state.selected = providers.len().saturating_sub(1);
            ProvidersKeyOutcome::Changed
        }
        KeyCode::Char('r') if key.modifiers.is_empty() => {
            state.provider_state.refresh();
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
        ref mut env_var_name,
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
                0 => env_var_name.push(c),
                1 => api_key.push(c),
                2 => base_url.push(c),
                _ => {}
            }
            ProvidersKeyOutcome::Changed
        }
        KeyCode::Backspace => {
            match *focused_field {
                0 => {
                    env_var_name.pop();
                }
                1 => {
                    api_key.pop();
                }
                2 => {
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
            let providers = state.provider_state.ordered_views();
            let save_outcome = providers.get(*provider_idx).map(|view| {
                (
                    view.id.0.clone(),
                    env_var_name.clone(),
                    api_key.clone(),
                    base_url.clone(),
                )
            });
            match save_outcome {
                Some((provider_id, env_name, key, url)) => ProvidersKeyOutcome::Save {
                    provider_id,
                    env_var_name: env_name,
                    api_key: key,
                    base_url: url,
                },
                None => ProvidersKeyOutcome::Changed,
            }
        }
        _ => ProvidersKeyOutcome::Unchanged,
    }
}

/// Validate a provider identifier.
fn validate_provider_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("provider ID cannot be empty".into());
    }
    if !id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err("provider ID must contain only letters, numbers, underscores, and hyphens".into());
    }
    Ok(())
}

/// Validate a base URL.
fn validate_base_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Ok(());
    }
    let parsed = url::Url::parse(url).map_err(|e| format!("invalid base URL: {e}"))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("base URL must use http or https scheme".into());
    }
    if parsed.host_str().unwrap_or_default().is_empty() {
        return Err("base URL must have a host".into());
    }
    Ok(())
}

/// Persist a provider configuration to disk using the repository's atomic
/// config editing facility. Validates provider ID and base URL before writing.
/// If `env_var_name` is non-empty, writes `env_key` and omits `api_key`.
/// Otherwise writes `api_key` as-is with an inline-key warning.
/// Called by the SaveProviderConfig effect handler and modals handler.
pub fn persist_provider_config(
    id: &str,
    env_var_name: &str,
    api_key: &str,
    base_url: &str,
) -> Result<(), String> {
    validate_provider_id(id)?;
    validate_base_url(base_url)?;

    if env_var_name.is_empty() && api_key.is_empty() && base_url.is_empty() {
        return Err("nothing to save".into());
    }

    let config_path = xai_grok_config::grok_home().join("config.toml");
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create config directory: {e}"))?;
    }
    let mut doc = read_config_document_for_edit(&config_path)
        .ok_or_else(|| "config.toml is not valid TOML; refusing to overwrite".to_string())?;
    let provider = doc
        .entry("provider")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .ok_or_else(|| "[provider] must be a table".to_string())?;
    let entry = provider
        .entry(id)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .ok_or_else(|| format!("[provider.{id}] must be a table").to_string())?;

    if !env_var_name.is_empty() {
        let mut arr = toml_edit::Array::new();
        arr.push(env_var_name);
        entry.insert("env_key", toml_edit::value(arr));
        entry.remove("api_key");
    } else if !api_key.is_empty() {
        entry["api_key"] = toml_edit::value(api_key);
    }
    if !base_url.is_empty() {
        entry["base_url"] = toml_edit::value(base_url);
    }
    std::fs::write(&config_path, doc.to_string())
        .map_err(|e| format!("cannot write config: {e}"))
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
            label: "r refresh",
            clickable: false,
            id: 2,
        },
        Shortcut {
            label: "Esc close",
            clickable: false,
            id: 3,
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
        format!(
            "{:<14} {:>5} {:>10}  {}",
            "Provider", "Models", "Status", "Endpoint"
        ),
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

    let providers = state.provider_state.ordered_views();
    let range = state.visible_range(row_area.height as usize);

    for (i, idx) in range.enumerate() {
        let Some(view) = providers.get(idx) else {
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

        let (status, status_color) = credential_status(view);
        let status_style = Style::default().fg(status_color).bg(bg);
        let row_style = Style::default().fg(fg).bg(bg);

        let models_label = if view.model_count > 0 {
            view.model_count.to_string()
        } else {
            "—".into()
        };
        let line = Line::from(vec![
            ratatui::text::Span::styled(format!(" {:<14}", view.display_name), row_style),
            ratatui::text::Span::styled(format!(" {:>5} ", models_label), row_style),
            ratatui::text::Span::styled(format!(" {:>10} ", status), status_style),
            ratatui::text::Span::styled(format!("  {}", view.endpoint), row_style),
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
            env_var_name,
            api_key,
            base_url,
            focused_field,
            show_api_key,
        } => (
            *provider_idx,
            env_var_name.clone(),
            api_key.clone(),
            base_url.clone(),
            *focused_field,
            *show_api_key,
        ),
        _ => return,
    };
    let (provider_idx, env_var_name_str, api_key_str, base_url_str, focused_field, show_api_key) = detail;

    let providers = state.provider_state.ordered_views();
    let Some(view) = providers.get(provider_idx) else {
        return;
    };

    let title = format!("Configure: {}", view.display_name);

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

    let env_display = if env_var_name_str.is_empty() {
        "(not set)".to_string()
    } else {
        env_var_name_str.clone()
    };
    render_field(
        buf,
        content.inner_x,
        render_y,
        content.inner_width,
        "Env Var Name:",
        &env_display,
        focused_field == 0,
        theme,
    );
    render_y += 1;

    let key_display = if show_api_key {
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
        "API Key:",
        &key_display,
        focused_field == 1,
        theme,
    );
    render_y += 1;

    render_field(
        buf,
        content.inner_x,
        render_y,
        content.inner_width,
        "Base URL:",
        &base_url_str,
        focused_field == 2,
        theme,
    );
    render_y += 1;

    if focused_field == 1 && !api_key_str.is_empty() {
        let warn_style = Style::default().fg(Color::Yellow);
        let warn_line = Line::from(vec![
            ratatui::text::Span::styled(
                "  \u{26a0}  ",
                warn_style,
            ),
            ratatui::text::Span::styled(
                "Warning: key stored in plaintext config; prefer Env Var Name",
                warn_style,
            ),
        ]);
        warn_line.render(
            Rect::new(content.inner_x, render_y, content.inner_width, 1),
            buf,
        );
        render_y += 1;
    }

    let (status, status_color) = credential_status(view);
    let status_style = Style::default().fg(status_color);
    let status_line = Line::from(vec![
        ratatui::text::Span::styled(
            "  Status:  ",
            Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
        ),
        ratatui::text::Span::styled(status, status_style),
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
        ratatui::text::Span::styled(&view.endpoint, Style::default().fg(theme.gray_bright)),
    ]);
    endpoint_line.render(
        Rect::new(content.inner_x, render_y, content.inner_width, 1),
        buf,
    );
    render_y += 1;

    render_y += 1;

    if let Some(ref error) = view.last_error {
        let error_style = Style::default().fg(Color::Red);
        let error_line = Line::from(vec![
            ratatui::text::Span::styled(
                "  Error:  ",
                Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::styled(error, error_style),
        ]);
        error_line.render(
            Rect::new(content.inner_x, render_y, content.inner_width, 1),
            buf,
        );
        render_y += 1;
    }

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
