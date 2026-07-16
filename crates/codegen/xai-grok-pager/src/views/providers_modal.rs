use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// State for the Providers configuration modal.
pub struct ProvidersModalState {
    pub selected: usize,
    pub scroll_offset: usize,
}

impl ProvidersModalState {
    pub fn new() -> Self {
        Self {
            selected: 0,
            scroll_offset: 0,
        }
    }
}

pub fn render_providers_modal(f: &mut Frame, _state: &mut ProvidersModalState) {
    let items = [
        ("xAI", "Connected", "api.x.ai"),
        ("OpenAI", "Not configured", ""),
        ("Anthropic", "Not configured", ""),
        ("OpenCode Zen", "Free tier", "opencode.ai"),
        ("Ollama", "Local", "localhost:11434"),
    ];

    let area = f.area();
    let block = Block::default()
        .title(" Providers ")
        .borders(Borders::ALL);
    let inner = block.inner(area);
    f.render_widget(block, area);

    for (i, (name, status, endpoint)) in items.iter().enumerate() {
        let y = inner.top() + i as u16;
        let line = format!(" {:<12} {:>12}  {}", name, status, endpoint);
        f.render_widget(
            Paragraph::new(line).style(Style::default()),
            Rect::new(inner.left(), y, inner.width, 1),
        );
    }
}
