mod support;

use support::TestRepo;

#[test]
fn testrepo_commits_are_ordered() {
    let repo = TestRepo::new();
    repo.commit("a");
    repo.commit("b");
    assert_eq!(repo.git(&["rev-list", "--count", "HEAD"]).trim(), "2");
    assert_eq!(repo.git(&["log", "--format=%s"]), "b\na\n");
}
