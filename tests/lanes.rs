use terminal_git_graph::git::log::Commit;
use terminal_git_graph::graph::lanes::{GraphRow, LaneBuilder, LaneSeg, PALETTE_LEN};

fn c(hash: &str, parents: &[&str]) -> Commit {
    Commit::new(hash, parents, 0, hash)
}

fn build(commits: &[Commit]) -> Vec<GraphRow> {
    let mut b = LaneBuilder::new();
    commits.iter().map(|x| b.push(x)).collect()
}

fn lanes(segs: &[LaneSeg]) -> Vec<usize> {
    segs.iter().map(|s| s.lane).collect()
}

#[test]
fn linear_history_stays_in_lane_0() {
    let rows = build(&[c("C", &["B"]), c("B", &["A"]), c("A", &[])]);
    for r in &rows {
        assert_eq!(r.lane, 0, "{}", r.hash);
        assert!(r.pass.is_empty() && r.joins.is_empty() && r.forks.is_empty());
        assert!(!r.is_merge);
        assert_eq!(r.width, 1);
    }
    assert!(rows[0].continues && rows[1].continues);
}

#[test]
fn fork_creates_second_lane_and_join_closes_it() {
    // B and C both branch off A.
    let rows = build(&[c("B", &["A"]), c("C", &["A"]), c("A", &[])]);
    assert_eq!(rows[0].lane, 0);
    assert_eq!(rows[1].lane, 1);
    assert_eq!(lanes(&rows[1].pass), vec![0]);
    assert_eq!(rows[1].width, 2);
    let a = &rows[2];
    assert_eq!(a.lane, 0);
    assert_eq!(a.joins.len(), 1);
    assert_eq!(a.joins[0].lane, 1);
    assert!(a.pass.is_empty());
}

#[test]
fn merge_commit_forks_to_second_parent() {
    let rows = build(&[c("M", &["A", "B"]), c("B", &["A"]), c("A", &[])]);
    let m = &rows[0];
    assert!(m.is_merge);
    assert!(m.continues);
    assert_eq!(m.forks.len(), 1);
    assert_eq!(m.forks[0].lane, 1);
    assert_eq!(m.width, 2);
    assert_eq!(rows[1].lane, 1);
    assert_eq!(lanes(&rows[2].joins), vec![1]);
}

#[test]
fn merge_into_lane_already_waiting_for_parent() {
    // M merges B, which lane 1 (from T) already waits for: no new lane.
    let rows = build(&[
        c("T", &["B"]),
        c("M", &["A", "B"]),
        c("B", &["A"]),
        c("A", &[]),
    ]);
    let m = &rows[1];
    assert_eq!(m.lane, 1);
    assert_eq!(
        m.forks,
        vec![LaneSeg {
            lane: 0,
            color: rows[0].color
        }]
    );
    assert_eq!(m.width, 2);
}

#[test]
fn closed_lane_is_reused_by_next_branch() {
    let rows = build(&[
        c("M", &["A", "B"]),
        c("B", &["A"]),
        c("A", &["R"]), // closes lane 1
        c("R", &["Q", "Z"]),
        c("Z", &["Q"]),
        c("Q", &[]),
    ]);
    assert_eq!(lanes(&rows[2].joins), vec![1]);
    assert_eq!(rows[2].width, 2);
    let r = &rows[3];
    assert_eq!(lanes(&r.forks), vec![1]);
    assert_eq!(r.width, 2);
    assert_eq!(rows[4].lane, 1);
}

#[test]
fn octopus_merge_with_three_parents() {
    let rows = build(&[
        c("O", &["A", "B", "C"]),
        c("C", &["A"]),
        c("B", &["A"]),
        c("A", &[]),
    ]);
    let o = &rows[0];
    assert!(o.is_merge);
    assert_eq!(o.forks.len(), 2);
    assert_eq!(lanes(&o.forks), vec![1, 2]);
    assert_eq!(o.width, 3);
    assert_eq!(rows[1].lane, 2);
    assert_eq!(rows[2].lane, 1);
    assert_eq!(lanes(&rows[3].joins), vec![1, 2]);
    for r in &rows[1..] {
        for p in &r.pass {
            assert!(!r.joins.contains(p), "{}: pass and joins overlap", r.hash);
            assert_ne!(p.lane, r.lane);
        }
    }
}

#[test]
fn lane_color_is_stable_until_join() {
    let rows = build(&[
        c("M", &["A", "B"]),
        c("X", &["A"]),
        c("B", &["B0"]),
        c("B0", &["A"]),
        c("A", &[]),
    ]);
    let branch = rows[0].forks[0];
    assert_ne!(branch.color, rows[0].color);
    assert_eq!(rows[2].color, branch.color);
    assert_eq!(rows[3].color, branch.color);
    let joined: Vec<_> = rows[4]
        .joins
        .iter()
        .filter(|s| s.lane == branch.lane)
        .collect();
    assert_eq!(joined[0].color, branch.color);
    // Colors cycle through the palette.
    for r in &rows {
        assert!(r.color < PALETTE_LEN);
    }
}

#[test]
fn new_lane_colors_cycle_through_palette() {
    let parents: Vec<String> = (0..PALETTE_LEN + 1).map(|i| format!("P{i}")).collect();
    let refs: Vec<&str> = parents.iter().map(|s| s.as_str()).collect();
    let rows = build(&[c("O", &refs)]);
    let colors: Vec<usize> = std::iter::once(rows[0].color)
        .chain(rows[0].forks.iter().map(|s| s.color))
        .collect();
    let expected: Vec<usize> = (0..PALETTE_LEN + 1).map(|i| i % PALETTE_LEN).collect();
    assert_eq!(colors, expected);
}

#[test]
fn incremental_push_equals_single_batch() {
    let commits = vec![
        c("O", &["A", "B", "C"]),
        c("T", &["B"]),
        c("C", &["A"]),
        c("B", &["A"]),
        c("A", &["R"]),
        c("R", &["Q", "Z"]),
        c("Z", &["Q"]),
        c("Q", &[]),
    ];
    let once = build(&commits);
    for split in 1..commits.len() {
        let mut b = LaneBuilder::new();
        let mut rows: Vec<GraphRow> = commits[..split].iter().map(|x| b.push(x)).collect();
        rows.extend(commits[split..].iter().map(|x| b.push(x)));
        assert_eq!(rows, once, "split at {split}");
    }
}

#[test]
fn root_commit_does_not_continue() {
    let rows = build(&[c("B", &["A"]), c("A", &[])]);
    assert!(rows[0].continues);
    assert!(!rows[1].continues);
    assert!(!rows[1].is_merge);
}
