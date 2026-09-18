use logscope::models::Level;
use logscope::core::evtx::{map_evtx_level, parse_evtx};

#[test]
fn test_map_evtx_level() {
    assert_eq!(map_evtx_level(1), Level::Critical);
    assert_eq!(map_evtx_level(2), Level::Error);
    assert_eq!(map_evtx_level(3), Level::Warning);
    assert_eq!(map_evtx_level(4), Level::Info);
    assert_eq!(map_evtx_level(5), Level::Debug);
    assert_eq!(map_evtx_level(0), Level::Unknown);
    assert_eq!(map_evtx_level(9), Level::Unknown);
}

#[test]
fn test_parse_evtx_garbage() {
    let result = parse_evtx("bad.evtx", b"not an evtx file at all");
    assert!(result.is_err());
}
