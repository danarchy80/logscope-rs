use logscope::core::workspace::Workspace;
use logscope::models::{Level, LogEntry, LogSource};

fn src(name: &str, n: usize) -> LogSource {
    let entries = (0..n).map(|i| LogEntry {
        timestamp: chrono::Utc::now(),
        raw_lines: vec![format!("line {i}")],
        source: name.to_string(),
        original_line_number: i,
        level: Level::Info,
    }).collect();
    LogSource { name: name.to_string(), entries }
}

#[test]
fn empty_workspace() {
    let w = Workspace::new();
    assert_eq!(w.len(), 0);
    assert!(w.is_empty());
    assert!(w.merged_sources().is_empty());
}

#[test]
fn add_and_merge() {
    let mut w = Workspace::new();
    w.add("a".to_string(), vec![src("s1", 2)]);
    w.add("b".to_string(), vec![src("s2", 1), src("s3", 1)]);
    assert_eq!(w.len(), 2);
    let merged = w.merged_sources();
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].name, "s1");
    assert_eq!(merged[1].name, "s2");
    assert_eq!(merged[2].name, "s3");
}

#[test]
fn remove_drops_only_that_input() {
    let mut w = Workspace::new();
    w.add("in1".to_string(), vec![src("app.log", 1)]);
    w.add("in2".to_string(), vec![src("app.log", 2)]);
    w.remove(0);
    let merged = w.merged_sources();
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].name, "app.log");
    assert_eq!(merged[0].entries.len(), 2);
}

#[test]
fn remove_out_of_range() {
    let mut w = Workspace::new();
    w.add("in1".to_string(), vec![src("app.log", 1)]);
    let res = w.remove(99);
    assert!(res.is_none());
    assert_eq!(w.len(), 1);
}

#[test]
fn contains() {
    let mut w = Workspace::new();
    w.add("exact_match".to_string(), vec![]);
    assert!(w.contains("exact_match"));
    assert!(!w.contains("different_path"));
}

#[test]
fn clear() {
    let mut w = Workspace::new();
    w.add("in1".to_string(), vec![src("app.log", 1)]);
    w.clear();
    assert!(w.is_empty());
    assert!(w.merged_sources().is_empty());
}
