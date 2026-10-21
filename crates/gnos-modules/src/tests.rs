use super::*;

#[test]
fn every_native_manifest_boots() {
    let r = boot(NATIVE, None);
    assert_eq!(r.refused, Vec::new(), "native manifests are valid");
    assert_eq!(r.booted.len(), NATIVE.len());
    for entry in NATIVE_ENTRIES {
        assert!(r.module(entry).is_some(), "{entry} has a manifest");
    }
    assert!(r.provides("clock", Place::Bar) && r.provides("clock", Place::Desktop));
    assert!(!r.provides("visualiser", Place::Bar));
}

#[test]
fn a_bad_manifest_is_refused_with_its_reason() {
    let base = NATIVE[0].1;
    let with = |from: &str, to: &str| base.replace(from, to);
    let cases = [
        (with("gnosix.module.v1", "gnosix.module.v9"), "schema"),
        (with("\"gnosix.workspaces\"", "\"Workspaces\""), "id"),
        (
            with("places = [\"bar\", \"workspace_manager\"]", "places = []"),
            "no place",
        ),
        (with("\"layout-grid\"", "\"no-such-icon\""), "icon"),
        (
            with("entry = \"workspaces\"", "entry = \"teapot\""),
            "no native module",
        ),
        (with("\"workspace_manager\"]", "\"ceiling\"]"), "unreadable"),
    ];
    for (text, why) in cases {
        let r = boot(&[("case", &text)], None);
        assert!(r.booted.is_empty(), "{why}");
        let reason = r.refused[0].1.to_string();
        assert!(reason.contains(why), "{why}: {reason}");
    }
    let r = boot(&[("a", base), ("b", base)], None);
    assert_eq!(r.booted.len(), 1);
    assert!(matches!(r.refused[0].1, Refusal::Duplicate(_)));
}

#[test]
fn mods_are_listed_and_never_booted() {
    let dir = std::env::temp_dir().join(format!("gnos-modules-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("weather")).unwrap();
    std::fs::write(
        dir.join("weather/module.toml"),
        r#"
schema = "gnosix.module.v1"
id = "example.weather"
name = "Weather"
icon = "cloud"
version = "0.1.0"
places = ["bar"]

[provider]
kind = "process"
command = "weather-module"
"#,
    )
    .unwrap();
    // A file that claims a native entry still does not run.
    std::fs::write(dir.join("clock.toml"), NATIVE[3].1).unwrap();
    std::fs::write(dir.join("broken.toml"), "schema = ").unwrap();
    std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
    let r = boot(NATIVE, Some(&dir));
    assert_eq!(r.booted.len(), NATIVE.len(), "mods add nothing");
    let found: Vec<(String, Option<String>, String)> = r
        .found
        .iter()
        .map(|f| {
            (
                f.origin.label(),
                f.manifest.as_ref().map(|m| m.id.clone()),
                f.refusal.to_string(),
            )
        })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found.len(), 3, "{found:?}");
    assert_eq!(found[0].0, "broken.toml");
    assert!(found[0].2.starts_with("unreadable"));
    assert_eq!(found[1].1.as_deref(), Some("gnosix.clock"));
    assert_eq!(found[1].2, "third-party modules do not run yet");
    assert_eq!(found[2].0, "module.toml");
    assert_eq!(found[2].1.as_deref(), Some("example.weather"));
    assert_eq!(found[2].2, "third-party modules do not run yet");
}
