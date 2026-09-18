use chrono::{DateTime, Utc};
use crate::models::{Level, LogEntry};

/// Severity rank for max-severity cell coloring. Unknown = 0 (empty cell).
fn severity_rank(l: Level) -> u8 {
    match l {
        Level::Critical => 6,
        Level::Error => 5,
        Level::Warning => 4,
        Level::Info => 3,
        Level::Debug => 2,
        Level::Trace => 1,
        Level::Unknown => 0,
    }
}

/// Hex color for a severity level (dark-theme palette). Unknown = grid bg.
pub fn level_color(level: Level) -> &'static str {
    match level {
        Level::Critical => "#7f1d1d",
        Level::Error => "#dc2626",
        Level::Warning => "#f59e0b",
        Level::Info => "#2563eb",
        Level::Debug => "#64748b",
        Level::Trace => "#94a3b8",
        Level::Unknown => "#1f2937",
    }
}

pub struct Heatmap {
    pub sources: Vec<String>,          // sorted alphabetically, deduped
    pub num_buckets: usize,
    pub min_ts: Option<DateTime<Utc>>, // None iff entries empty
    pub max_ts: Option<DateTime<Utc>>,
    pub cells: Vec<Vec<Level>>,        // [source_index][bucket_index]
}

/// Build a heatmap from entries. Timeline = [min_ts, max_ts] of the entries
/// (data-driven). Sources sorted alphabetically (stable, deterministic SVG).
/// Each cell = max severity (by severity_rank) of entries in that bucket.
/// Empty entries -> empty sources, num_buckets preserved, min/max None.
pub fn build_heatmap(entries: &[LogEntry], num_buckets: usize) -> Heatmap {
    if entries.is_empty() {
        return Heatmap {
            sources: vec![],
            num_buckets,
            min_ts: None,
            max_ts: None,
            cells: vec![],
        };
    }

    let min_ts = entries.iter().map(|e| e.timestamp).min().unwrap();
    let max_ts = entries.iter().map(|e| e.timestamp).max().unwrap();

    let mut sources: Vec<String> = entries.iter().map(|e| e.source.clone()).collect();
    sources.sort();
    sources.dedup();

    let mut cells = vec![vec![Level::Unknown; num_buckets]; sources.len()];

    let range_ms = max_ts.signed_duration_since(min_ts).num_milliseconds();

    for entry in entries {
        let bucket = if range_ms == 0 {
            0
        } else {
            let offset = entry.timestamp.signed_duration_since(min_ts).num_milliseconds();
            let b = (offset * num_buckets as i64 / range_ms) as usize;
            b.min(num_buckets.saturating_sub(1))
        };

        let src_idx = sources.binary_search(&entry.source).unwrap();
        
        let current_level = cells[src_idx][bucket];
        if severity_rank(entry.level) > severity_rank(current_level) {
            cells[src_idx][bucket] = entry.level;
        }
    }

    Heatmap {
        sources,
        num_buckets,
        min_ts: Some(min_ts),
        max_ts: Some(max_ts),
        cells,
    }
}

/// Render the heatmap as a standalone SVG string.
pub fn render_heatmap_svg(h: &Heatmap) -> String {
    let cell_w = 8;
    let row_h = 18;
    let margin_left = 160;
    let margin_top = 30;
    let margin_right_bottom = 24;

    let total_width = margin_left + h.num_buckets * cell_w + margin_right_bottom;
    let total_height = margin_top + h.sources.len() * row_h + 40;

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" font-family="monospace">"##,
        total_width, total_height
    ));
    svg.push_str(&format!(
        r##"<rect width="{}" height="{}" fill="#0b1220" />"##,
        total_width, total_height
    ));

    if h.min_ts.is_none() {
        svg.push_str(r##"<text x="50%" y="50%" text-anchor="middle" dominant-baseline="middle" fill="#9ca3af">no events</text>"##);
        svg.push_str("</svg>");
        return svg;
    }

    svg.push_str(&format!(
        r##"<text x="16" y="20" fill="#f9fafb">LogScope heatmap — {} sources × {} buckets</text>"##,
        h.sources.len(), h.num_buckets
    ));


    for (i, source) in h.sources.iter().enumerate() {
        let display_source = if source.chars().count() > 30 {
            let mut s: String = source.chars().take(29).collect();
            s.push('…');
            s
        } else {
            source.clone()
        };

        let y_center = margin_top + i * row_h + row_h / 2;
        svg.push_str(&format!(
            r##"<text x="{}" y="{}" text-anchor="end" dominant-baseline="middle" fill="#e5e7eb" font-size="12">{}</text>"##,
            margin_left - 8, y_center, display_source
        ));

        for j in 0..h.num_buckets {
            let lvl = h.cells[i][j];
            if lvl != Level::Unknown {
                let color = level_color(lvl);
                let x = margin_left + j * cell_w;
                let y = margin_top + i * row_h;
                svg.push_str(&format!(
                    r##"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" />"##,
                    x, y, cell_w - 1, row_h - 1, color
                ));
            }
        }
    }

    let min_ts = h.min_ts.unwrap();
    let max_ts = h.max_ts.unwrap();
    let mid_ts = min_ts + chrono::Duration::milliseconds(max_ts.signed_duration_since(min_ts).num_milliseconds() / 2);

    let format_ts = |ts: DateTime<Utc>| ts.format("%Y-%m-%d %H:%M").to_string();
    
    let axis_y = margin_top + h.sources.len() * row_h + 12;
    svg.push_str(&format!(
        r##"<text x="{}" y="{}" text-anchor="start" fill="#9ca3af" font-size="10">{}</text>"##,
        margin_left, axis_y, format_ts(min_ts)
    ));
    svg.push_str(&format!(
        r##"<text x="{}" y="{}" text-anchor="middle" fill="#9ca3af" font-size="10">{}</text>"##,
        margin_left + (h.num_buckets * cell_w) / 2, axis_y, format_ts(mid_ts)
    ));
    svg.push_str(&format!(
        r##"<text x="{}" y="{}" text-anchor="end" fill="#9ca3af" font-size="10">{}</text>"##,
        margin_left + h.num_buckets * cell_w, axis_y, format_ts(max_ts)
    ));

    let legend_y = total_height - 16;
    let mut legend_x = margin_left;
    let legend_levels = [
        Level::Critical,
        Level::Error,
        Level::Warning,
        Level::Info,
        Level::Debug,
        Level::Trace,
    ];

    for lvl in legend_levels.iter() {
        let color = level_color(*lvl);
        let label = match lvl {
            Level::Critical => "Critical",
            Level::Error => "Error",
            Level::Warning => "Warning",
            Level::Info => "Info",
            Level::Debug => "Debug",
            Level::Trace => "Trace",
            Level::Unknown => "",
        };

        svg.push_str(&format!(
            r##"<rect x="{}" y="{}" width="12" height="12" fill="{}" />"##,
            legend_x, legend_y - 10, color
        ));
        svg.push_str(&format!(
            r##"<text x="{}" y="{}" fill="#e5e7eb" font-size="12">{}</text>"##,
            legend_x + 16, legend_y, label
        ));
        legend_x += 80;
    }

    svg.push_str("</svg>");
    svg
}
