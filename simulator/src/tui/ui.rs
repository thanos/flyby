//! Ratatui widgets for the simulator dashboard.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Sparkline};

use super::app::{DashState, RunMode};

pub fn draw(frame: &mut Frame<'_>, state: &DashState) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(7),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_header(frame, root[0], state);
    draw_clock(frame, root[1], state);

    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(root[2]);
    draw_stats(frame, mid[0], state);
    draw_events(frame, mid[1], state);

    draw_charts(frame, root[3], state);
    draw_footer(frame, root[4], state);
}

fn draw_header(frame: &mut Frame<'_>, area: Rect, state: &DashState) {
    let mode = match state.mode {
        RunMode::Auto => Span::styled(
            " AUTO ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        RunMode::Paused => Span::styled(
            " PAUSED ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    };
    let fin = if state.finished {
        Span::styled(" DONE ", Style::default().fg(Color::Black).bg(Color::Cyan))
    } else {
        Span::raw("")
    };
    let title = Line::from(vec![
        Span::styled(
            " FlyBy Simulator ",
            Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        mode,
        fin,
        Span::raw("  "),
        Span::styled(
            state.scenario_name.clone(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — "),
        Span::raw(state.scenario_desc.clone()),
        Span::raw("  "),
        Span::styled("[SIMULATED]", Style::default().fg(Color::Magenta)),
    ]);
    frame.render_widget(
        Paragraph::new(title).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn draw_clock(frame: &mut Frame<'_>, area: Rect, state: &DashState) {
    let ratio = if state.duration_ns == 0 {
        0.0
    } else {
        (state.clock_ns as f64 / state.duration_ns as f64).clamp(0.0, 1.0)
    };
    let label = format!(
        "clock {:.3} ms / {:.3} ms   tick={}   speed×{}",
        state.clock_ns as f64 / 1e6,
        state.duration_ns as f64 / 1e6,
        state.stats.ticks,
        state.ticks_per_frame
    );
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Simulator clock"),
        )
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
        .ratio(ratio)
        .label(label);
    frame.render_widget(gauge, area);
}

fn draw_stats(frame: &mut Frame<'_>, area: Rect, state: &DashState) {
    let (ring_line, ring_gauge_ratio) = match state.ring {
        Some((len, cap, occ)) => (
            format!("ring      {len}/{cap}  ({:.0}% full)", occ * 100.0),
            occ.clamp(0.0, 1.0),
        ),
        None => ("ring      (none)".into(), 0.0),
    };

    let s = &state.stats;
    let drop_pct = if s.packets_generated == 0 {
        0.0
    } else {
        100.0 * s.packets_dropped as f64 / s.packets_generated as f64
    };
    let lines = vec![
        Line::from(format!(
            "packets   gen={}  drop={} ({drop_pct:.1}%)  corrupt={}",
            s.packets_generated, s.packets_dropped, s.packets_corrupted
        )),
        Line::from(format!(
            "shm       written={}  consumed={}  overflow={}",
            s.slots_written, s.slots_consumed, s.ring_overflows
        )),
        Line::from(format!("consumer  reads={}", state.consumer_reads)),
        Line::from(ring_line),
        Line::from(format!("batch     last_len={}", state.last_batch_len)),
        Line::from(format!("wall      {:?}", s.elapsed)),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(3)])
        .split(area);

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Pipeline / queues"),
        ),
        chunks[0],
    );

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Ring occupancy"),
        )
        .gauge_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray))
        .ratio(ring_gauge_ratio);
    frame.render_widget(gauge, chunks[1]);
}

fn draw_events(frame: &mut Frame<'_>, area: Rect, state: &DashState) {
    let height = area.height.saturating_sub(2) as usize;
    let start = state.event_log.len().saturating_sub(height.max(1));
    let items: Vec<ListItem<'_>> = state.event_log[start..]
        .iter()
        .map(|line| {
            let style = if line.contains("DROP") || line.contains("OVERFLOW") {
                Style::default().fg(Color::Red)
            } else if line.contains("CORRUPT") {
                Style::default().fg(Color::Yellow)
            } else if line.contains("tick=") {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Line::from(Span::styled(line.clone(), style)))
        })
        .collect();

    frame.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Event flow (newest at bottom)"),
        ),
        area,
    );
}

fn draw_charts(frame: &mut Frame<'_>, area: Rect, state: &DashState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let pps: Vec<u64> = state.pps_hist.clone();
    let lat: Vec<u64> = state.tick_lat_hist.clone();

    frame.render_widget(
        Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Packets / tick (simulated)"),
            )
            .data(&pps)
            .style(Style::default().fg(Color::Green)),
        cols[0],
    );
    frame.render_widget(
        Sparkline::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Tick wall latency (ns)"),
            )
            .data(&lat)
            .style(Style::default().fg(Color::LightBlue)),
        cols[1],
    );
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect, state: &DashState) {
    let line = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::raw(state.status.clone()),
        Span::raw("   "),
        Span::styled(
            "Space",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" run/pause  "),
        Span::styled(
            "s",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" step  "),
        Span::styled(
            "+/-",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" speed  "),
        Span::styled(
            "r",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" restart  "),
        Span::styled(
            "q",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" quit"),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}
