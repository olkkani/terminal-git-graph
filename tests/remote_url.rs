use terminal_git_graph::remote_url::to_https;

#[test]
fn scp_style_git_at_host() {
    assert_eq!(
        to_https("git@github.com:o/r.git").as_deref(),
        Some("https://github.com/o/r")
    );
    assert_eq!(
        to_https("git@gitlab.com:group/sub/r").as_deref(),
        Some("https://gitlab.com/group/sub/r")
    );
}

#[test]
fn ssh_scheme() {
    assert_eq!(
        to_https("ssh://git@github.com/o/r.git").as_deref(),
        Some("https://github.com/o/r")
    );
    // The SSH port means nothing to the web server.
    assert_eq!(
        to_https("ssh://git@example.com:2222/o/r.git").as_deref(),
        Some("https://example.com/o/r")
    );
    assert_eq!(
        to_https("git://example.com/o/r.git").as_deref(),
        Some("https://example.com/o/r")
    );
}

#[test]
fn https_with_dot_git() {
    assert_eq!(
        to_https("https://github.com/o/r.git").as_deref(),
        Some("https://github.com/o/r")
    );
    assert_eq!(
        to_https("https://github.com/o/r/").as_deref(),
        Some("https://github.com/o/r")
    );
    assert_eq!(
        to_https("https://example.com:8443/o/r").as_deref(),
        Some("https://example.com:8443/o/r")
    );
}

#[test]
fn https_with_credentials_are_stripped() {
    assert_eq!(
        to_https("https://user:token@github.com/o/r.git").as_deref(),
        Some("https://github.com/o/r")
    );
    assert_eq!(
        to_https("https://user@github.com/o/r").as_deref(),
        Some("https://github.com/o/r")
    );
}

#[test]
fn non_url_returns_none() {
    for s in [
        "",
        "/srv/git/r.git",
        "../r",
        "file:///srv/git/r.git",
        "C:\\repos\\r",
        "github.com",
        "git@github.com:",
        "https://",
        "has space:o/r",
    ] {
        assert_eq!(to_https(s), None, "{s:?}");
    }
}
