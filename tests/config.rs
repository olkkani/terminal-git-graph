use std::fs;

use terminal_git_graph::config::{default_config_path, Config};

#[test]
fn missing_file_gives_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let (config, warning) = Config::load(&tmp.path().join("nope.toml"));
    assert_eq!(config.scan_depth, 3);
    assert!(!config.show_tags);
    assert_eq!(config.theme, "default");
    assert_eq!(warning, None);
}

#[test]
fn values_are_read() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    fs::write(
        &path,
        "scan_depth = 2\nshow_tags = true\ntheme = \"dark\"\n",
    )
    .unwrap();
    let (config, warning) = Config::load(&path);
    assert_eq!(
        (config.scan_depth, config.show_tags, config.theme.as_str()),
        (2, true, "dark")
    );
    assert_eq!(warning, None);
}

#[test]
fn depth_is_clamped_to_1_5() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    for (written, want) in [
        ("0", 1),
        ("-4", 1),
        ("1", 1),
        ("5", 5),
        ("9", 5),
        ("1000", 5),
    ] {
        fs::write(&path, format!("scan_depth = {written}\n")).unwrap();
        let (config, warning) = Config::load(&path);
        assert_eq!(config.scan_depth, want, "scan_depth = {written}");
        assert_eq!(warning, None);
    }
}

#[test]
fn invalid_toml_gives_defaults_and_warning() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    for bad in ["scan_depth = ", "show_tags = \"yes\"\n"] {
        fs::write(&path, bad).unwrap();
        let (config, warning) = Config::load(&path);
        assert_eq!(config.scan_depth, 3);
        assert!(!config.show_tags);
        assert_eq!(config.theme, "default");
        let warning = warning.expect("warning");
        assert!(warning.contains("config.toml"), "{warning}");
    }
}

#[test]
fn default_path_is_under_dot_config() {
    let path = default_config_path();
    assert!(
        path.ends_with(".config/terminal-git-graph/config.toml"),
        "{}",
        path.display()
    );
}
