use agentloop_contracts::PermissionRule;

#[test]
fn parses_bare_and_specified_rules() {
    let cases = [
        ("WebSearch", ("WebSearch", None)),
        ("Bash(git *)", ("Bash", Some("git *"))),
        ("Read(~/secrets/**)", ("Read", Some("~/secrets/**"))),
        ("Edit(/src/**)", ("Edit", Some("/src/**"))),
        (
            "WebFetch(domain:example.com)",
            ("WebFetch", Some("domain:example.com")),
        ),
        ("Bash(echo (hi))", ("Bash", Some("echo (hi)"))),
        ("  Glob  ", ("Glob", None)),
    ];
    for (raw, (tool, spec)) in cases {
        let rule = PermissionRule::parse(raw).unwrap_or_else(|| panic!("should parse: {raw}"));
        assert_eq!(rule.tool, tool);
        assert_eq!(rule.specifier.as_deref(), spec);
    }
}

#[test]
fn rejects_malformed_rules() {
    for raw in ["", "   ", "(git *)", "Bash(git *", "Bash)oops("] {
        assert!(
            PermissionRule::parse(raw).is_none(),
            "should reject: {raw:?}"
        );
    }
}

#[test]
fn display_roundtrips() {
    for raw in ["WebSearch", "Bash(git *)", "Read(~/secrets/**)"] {
        let rule = PermissionRule::parse(raw).expect("parse");
        assert_eq!(rule.to_string(), raw);
    }
}

#[test]
fn serde_uses_rule_syntax() {
    let rule: PermissionRule = serde_json::from_str("\"Bash(npm run *)\"").expect("deserialize");
    assert_eq!(rule.tool, "Bash");
    assert_eq!(
        serde_json::to_string(&rule).expect("serialize"),
        "\"Bash(npm run *)\""
    );
    assert!(serde_json::from_str::<PermissionRule>("\"(bad)\"").is_err());
}
