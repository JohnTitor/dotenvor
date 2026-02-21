use std::collections::BTreeMap;

use dotenvor::parse_str;

#[test]
fn parses_node_style_fixture() {
    let fixture = include_str!("fixtures/node-basic.env");
    let entries = parse_str(fixture).expect("fixture should parse");

    let map = to_map(entries);
    assert_eq!(map.get("BASIC").expect("BASIC"), "basic");
    assert_eq!(map.get("EMPTY").expect("EMPTY"), "");
    assert_eq!(map.get("INLINE_COMMENT").expect("INLINE_COMMENT"), "value");
    assert_eq!(map.get("QUOTED").expect("QUOTED"), "hello world");
}

#[test]
fn parses_godotenv_style_export_fixture() {
    let fixture = include_str!("fixtures/go-export.env");
    let entries = parse_str(fixture).expect("fixture should parse");

    let map = to_map(entries);
    assert_eq!(map.get("EXPORTED").expect("EXPORTED"), "1");
    assert_eq!(map.get("WITH_SPACES").expect("WITH_SPACES"), "a b c");
}

#[test]
fn parses_node_style_multiline_fixture() {
    let fixture = include_str!("fixtures/node-multiline.env");
    let entries = parse_str(fixture).expect("fixture should parse");

    let map = to_map(entries);
    assert_eq!(
        map.get("MULTI_DOUBLE_QUOTED").expect("MULTI_DOUBLE_QUOTED"),
        "THIS\nIS\nA\nMULTILINE\nSTRING"
    );
    assert_eq!(
        map.get("MULTI_SINGLE_QUOTED").expect("MULTI_SINGLE_QUOTED"),
        "THIS\nIS\nA\nMULTILINE\nSTRING"
    );
    assert_eq!(
        map.get("MULTI_BACKTICKED").expect("MULTI_BACKTICKED"),
        "THIS\nIS\nA\n\"MULTILINE'S\"\nSTRING"
    );
    assert_eq!(
        map.get("MULTI_PEM_DOUBLE_QUOTED")
            .expect("MULTI_PEM_DOUBLE_QUOTED"),
        "-----BEGIN PUBLIC KEY-----\nLINE1\nLINE2\n-----END PUBLIC KEY-----"
    );
    assert_eq!(map.get("AFTER").expect("AFTER"), "after_line");
}

fn to_map(entries: Vec<dotenvor::Entry>) -> BTreeMap<String, String> {
    entries
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect()
}
