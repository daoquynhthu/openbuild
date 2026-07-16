use ratatui::layout::Rect;
use ratatui::widgets::Block;
use ratatui::Frame;

use crate::app::actions::Action;
use crate::settings::registry::SettingsRegistry;
use crate::views::modal::ModalContentArea;
use xai_grok_pager_render::modal_window_state::{ModalSizing, ModalWindowConfig, ModalWindowState, Shortcut};

/// State for the Providers configuration modal.
pub struct ProvidersModalState {
    pub window: ModalWindowState,
    pub selected: usize,
    pub scroll_offset: usize,
}

impl ProvidersModalState {
    pub fn new() -> Self {
        Self {
            window: ModalWindowState::new(),
            selected: 0,
            scroll_offset: 0,
        }
    }
}

pub fn render_providers_modal(
    f: &mut Frame,
    state: &mut ProvidersModalState,
    registry: &SettingsRegistry,
) {
    let shortcuts = vec![
        Shortcut { label: "Esc", clickable: true, id: 0 },
    ];

    let config = ModalWindowConfig {
        title: "Providers",
        tabs: None,
        shortcuts: &shortcuts,
        sizing: ModalSizing::medium(),
        fold_info: None,
    };

    let ModalContentArea { content, footer, .. } =
        xai_grok_pager_render::modal_window::render_modal_window(f, config, &mut state.window);

    let area = content;
    let _ = registry;
    let _ = footer;

    // Simple list rendering
    let items = vec![
        ("xAI", "Connected", "api.x.ai"),
        ("OpenAI", "Not configured", ""),
        ("Anthropic", "Not configured", ""),
        ("OpenCode Zen", "Free tier", "opencode.ai"),
        ("Ollama", "Local", "localhost:11434"),
    ];

    let block = Block::bordered().title(" Providers ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    for (i, (name, status, endpoint)) in items.iter().enumerate() {
        if i < state.scroll_offset {
            continue;
        }
        let y = inner.top() + (i - state.scroll_offset) as u16;
        if y >= inner.bottom() {
            break;
        }
        let line = format!(" {:<12} {:>12}  {}", name, status, endpoint);
        let style = if i == state.selected {
            ratatui::style::Style::default().bg(ratatui::style::Color::DarkGray)
        } else {
            ratatui::style::Style::default()
        };
        f.render_widget(
            ratatui::widgets::Paragraph::new(line).style(style),
            Rect::new(inner.left(), y, inner.width(), 1),
        );
    }
}
