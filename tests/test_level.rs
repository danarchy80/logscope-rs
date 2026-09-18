use logscope::models::Level;
use logscope::core::level::detect_level;

#[test]
fn test_detect_level() {
    // Syslog
    assert_eq!(detect_level(&["<3>".to_string()]), Level::Error);
    assert_eq!(detect_level(&["<0>".to_string()]), Level::Critical);
    assert_eq!(detect_level(&["<4>".to_string()]), Level::Warning);
    assert_eq!(detect_level(&["<6>".to_string()]), Level::Info);
    assert_eq!(detect_level(&["<7>".to_string()]), Level::Debug);

    // Keyword
    assert_eq!(detect_level(&["ERROR".to_string()]), Level::Error);
    assert_eq!(detect_level(&["WARN".to_string()]), Level::Warning);
    assert_eq!(detect_level(&["CRITICAL".to_string()]), Level::Critical);
    assert_eq!(detect_level(&["warning".to_string()]), Level::Warning);
    assert_eq!(detect_level(&["INFO".to_string()]), Level::Info);
    assert_eq!(detect_level(&["DEBUG".to_string()]), Level::Debug);

    // Priority
    assert_eq!(detect_level(&["CRITICAL ERROR".to_string()]), Level::Critical);

    // None
    assert_eq!(detect_level(&["just some text".to_string()]), Level::Unknown);
}

#[test]
fn test_level_parse() {
    assert_eq!(Level::parse("error"), Some(Level::Error));
    assert_eq!(Level::parse("warn"), Some(Level::Warning));
    assert_eq!(Level::parse("warning"), Some(Level::Warning));
    assert_eq!(Level::parse("critical"), Some(Level::Critical));
    assert_eq!(Level::parse("fatal"), Some(Level::Critical));
    assert_eq!(Level::parse("info"), Some(Level::Info));
    assert_eq!(Level::parse("bogus"), None);
}
