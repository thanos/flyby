//! Headless TUI frame capture for documentation screenshots.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::scenario::Scenario;

use super::app::Dashboard;
use super::ui;

/// Render a dashboard frame to a plain-text terminal dump after `steps` ticks.
pub fn render_text_frame(
    scenario: Scenario,
    steps: usize,
    width: u16,
    height: u16,
) -> flyby_core::Result<String> {
    let mut dash = Dashboard::new(scenario)?;
    for _ in 0..steps {
        dash.step_once()?;
        if dash.snapshot().finished {
            break;
        }
    }
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).map_err(|e| {
        flyby_core::Error::new(flyby_core::ErrorKind::Io, format!("tui snapshot: {e}"))
    })?;
    let state = dash.snapshot();
    terminal
        .draw(|frame| ui::draw(frame, &state))
        .map_err(|e| {
            flyby_core::Error::new(flyby_core::ErrorKind::Io, format!("tui snapshot: {e}"))
        })?;
    Ok(clean_backend_text(&terminal.backend().to_string()))
}

fn clean_backend_text(raw: &str) -> String {
    raw.lines()
        .map(|line| {
            let line = line.trim();
            let line = line.strip_prefix('"').unwrap_or(line);
            let line = line.strip_suffix('"').unwrap_or(line);
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert a TestBackend-style text dump into a dark-terminal SVG screenshot.
pub fn text_frame_to_svg(frame: &str, title: &str) -> String {
    let lines: Vec<&str> = frame.lines().collect();
    let cols = lines.iter().map(|l| l.chars().count()).max().unwrap_or(80);
    let rows = lines.len().max(1);
    let char_w = 7.2_f64;
    let char_h = 14.0_f64;
    let pad = 16.0;
    let width = pad * 2.0 + cols as f64 * char_w;
    let height = pad * 2.0 + 28.0 + rows as f64 * char_h;

    let mut out = String::new();
    out.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.1} {height:.1}">
  <rect width="100%" height="100%" fill="#0d1117" rx="8"/>
  <text x="{pad}" y="18" fill="#8b949e" font-family="ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace" font-size="11">{title}</text>
  <g font-family="ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace" font-size="12" fill="#c9d1d9">
"##
    ));

    for (i, line) in lines.iter().enumerate() {
        let y = pad + 28.0 + (i as f64 + 1.0) * char_h;
        let escaped = xml_escape(line);
        // Colour hints for key markers in the dump.
        let fill = if line.contains("DROP") || line.contains("OVERFLOW") {
            "#ff7b72"
        } else if line.contains("PAUSED") || line.contains("SIMULATED") {
            "#ffa657"
        } else if line.contains("AUTO") || line.contains("DONE") {
            "#3fb950"
        } else {
            "#c9d1d9"
        };
        out.push_str(&format!(
            r#"    <text x="{pad}" y="{y:.1}" fill="{fill}" xml:space="preserve">{escaped}</text>
"#
        ));
    }
    out.push_str("  </g>\n</svg>\n");
    out
}

fn xml_escape(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\u{0000}'..='\u{001f}' => " ".into(), // control chars from box drawing fallbacks
            c => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn render_paused_frame_contains_scenario() {
        let text = render_text_frame(
            Scenario {
                duration: Duration::from_millis(5),
                tick_ns: 1_000_000,
                ..Scenario::constant_rate()
            },
            0,
            100,
            28,
        )
        .unwrap();
        assert!(text.contains("constant_rate") || text.contains("FlyBy"));
        assert!(text.contains("SIMULATED") || text.contains("PAUSED") || text.contains("clock"));
    }

    #[test]
    fn render_steps_until_finished() {
        let text = render_text_frame(
            Scenario {
                duration: Duration::from_millis(3),
                tick_ns: 1_000_000,
                ..Scenario::constant_rate()
            },
            64,
            80,
            24,
        )
        .unwrap();
        assert!(!text.is_empty());
    }

    #[test]
    fn clean_backend_text_strips_quotes() {
        let cleaned = clean_backend_text("\"hello\"\n  \"world\"  \n");
        assert!(cleaned.contains("hello"));
        assert!(cleaned.contains("world"));
        assert!(!cleaned.contains('"'));
    }

    #[test]
    fn text_frame_to_svg_colors_and_escapes() {
        let frame = "PAUSED SIMULATED\nDROP overflow\nAUTO DONE\nplain & <tag>\nOVERFLOW ring";
        let svg = text_frame_to_svg(frame, "FlyBy shot");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("FlyBy shot"));
        assert!(svg.contains("#ffa657")); // paused/simulated
        assert!(svg.contains("#ff7b72")); // drop/overflow
        assert!(svg.contains("#3fb950")); // auto/done
        assert!(svg.contains("&amp;"));
        assert!(svg.contains("&lt;"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn xml_escape_control_chars() {
        assert_eq!(xml_escape("a\u{0001}b"), "a b");
        assert_eq!(xml_escape("\"x\""), "&quot;x&quot;");
        assert_eq!(xml_escape("a>b"), "a&gt;b");
    }
}
