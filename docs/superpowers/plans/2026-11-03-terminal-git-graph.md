# terminal-git-graph Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 터미널 어디서나 동작하는 Git 그래프 TUI(`tgg`)를 만든다. 원본 herdr-git-graph의 기능(그래프, 상세/diff, 실시간 갱신, Git 동작, Manygit)을 모두 포함한다.

**Architecture:** 하나의 Rust 크레이트(라이브러리 `terminal_git_graph` + 얇은 바이너리 `tgg`). `git` CLI를 인자 목록으로 호출하는 `git` 계층, 순수 함수인 `graph`(레인 계산, 글리프 렌더), 상태 머신인 `app`과 ratatui 기반 `ui`, 파일 감시 `watch`, 여러 저장소용 `manygit`으로 나눈다. 순수 로직과 git 동작은 실제 임시 저장소로 통합 테스트하고, UI는 ratatui `TestBackend` 버퍼로 검증한다.

**Tech Stack:** Rust(stable), ratatui(+crossterm), notify, clap(derive), serde+toml, thiserror, base64, unicode-width, 개발용 tempfile, 라이선스 검사용 cargo-deny.

**Spec:** `docs/superpowers/specs/2026-11-03-terminal-git-graph-design.md`

**Setup (구현 시작 전):** CLAUDE.md 규칙에 따라 새 worktree에서 작업한다: `git -C ../develop pull --ff-only origin develop` → `git -C ../.bare worktree add -b feature/mvp ../feature-mvp develop`.

## Global Constraints

- 프로젝트 라이선스 MIT. 원본(herdr-git-graph, vscode-git-graph) 소스를 복사/번역하지 않는다. 복사가 불가피하면 그 부분에 MIT 저작권 표기를 남긴다.
- 의존성은 MIT/Apache-2.0/BSD/ISC만 허용, GPL 계열 금지. `cargo deny check licenses`가 통과해야 한다. (계획상의 판단: 순수 허용형인 `Unicode-3.0`, `Zlib`, `CC0-1.0`도 deny.toml에 허용한다. 표준 크레이트가 요구한다.)
- 이름: 프로젝트 `terminal-git-graph`, 바이너리 `tgg`. "Herdr", "Git Graph"를 이름이나 로고에 쓰지 않는다. 원본 `preview.png`를 쓰지 않는다. README에 "영감을 받음"으로 출처를 밝힌다.
- git은 시스템 `git` CLI를 **셸 없이 인자 목록으로** 호출한다. 사용자가 입력한 이름은 `-`로 시작할 수 없다.
- 네트워크 접근은 Fetch, Fetch all, Pull, Push, 원격 브랜치 삭제를 명시 실행할 때만이다. 텔레메트리/업데이트 확인 없음.
- 터미널 전용 프로토콜 금지(ghostty/herdr 전용 기능 의존 불가). 마우스와 키보드 모두 지원. Nerd Font 없이 동작(유니코드), ASCII 대체 가능. 트루컬러가 없으면 256색으로 대체.
- 확인 대화상자는 **로컬 브랜치 체크아웃과 Fetch를 제외한 모든 변경 동작**에 필수. 위험한 선택은 빨간색. git이 거부하면 git의 메시지를 그대로 보여 준다.
- 충돌로 멈춘 merge/rebase/cherry-pick/revert는 git 오류 메시지만 보여 주고 끝낸다(continue/abort UI 없음).
- Uncommitted reset은 tracked 파일만 `git reset --hard`. untracked는 건드리지 않고 `git clean`은 제공하지 않는다. 확인 대화상자에 "Untracked files are kept." 문구를 넣는다.
- 첫 페이지 로드: 커밋 300개(`PAGE_SIZE = 300`), 끝까지 스크롤하면 300개씩 추가. 수만 커밋 저장소의 콜드 스타트 첫 화면이 약 1초 이내(최근 Apple Silicon Mac).
- Manygit: 탐색 깊이 기본 3(범위 1~5), 숨김 폴더와 `node_modules`/`build`/`dist`/`vendor`/`target` 건너뜀, 홈 폴더와 그 상위는 스캔하지 않음, 목록이 보이는 동안 15초마다 상태 갱신.
- 설정 파일 하나: `~/.config/terminal-git-graph/config.toml`(Manygit 깊이, 태그 표시, 색 테마). 설정 UI 없음.
- macOS 우선, Linux는 가능하면. Windows 비목표. UI 문자열은 영어.
- 40~60칸 폭이 우선 설계 대상이지만 하드 요구사항은 아니다.

## Review Focus

스펙이 암시하지만 개별 기능 테스트가 놓치기 쉬운 입력(가능성 높은 순). 각 줄은 해당 태스크에 테스트가 있다.

1. **커밋이 없는 저장소(unborn HEAD)**: 오류 없이 "No commits yet" 표시. (Task 3, 8)
2. **detached HEAD**: HEAD 라벨을 보여 주고, 현재 브랜치가 필요한 메뉴 항목은 비활성. (Task 3, 13)
3. **3개 이상 부모를 가진 octopus merge**: 레인이 깨지지 않는다. (Task 4)
4. **CJK/이모지 등 폭이 넓은 문자가 든 제목과 작성자**: 표시 폭 기준으로 자르고 열이 어긋나지 않는다. (Task 8)
5. **아주 작은 터미널과 리사이즈**: 패닉 없이 "Terminal too small" 안내. (Task 8)

---

### Task 1: 프로젝트 골격, 라이선스 검사, 테스트 fixture

**Files:**
- Create: `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `LICENSE`, `deny.toml`, `.gitignore`, `README.md`(제목과 한 줄 소개만), `tests/support/mod.rs`, `tests/smoke.rs`

**Interfaces:**
- Produces (`tests/support/mod.rs`, 이후 모든 통합 테스트가 `mod support;`로 사용):
  ```rust
  pub struct TestRepo { /* tempdir */ }
  impl TestRepo {
      pub fn new() -> Self;   // git init -b main, user.name "Test User", user.email "test@example.com"
      pub fn path(&self) -> &std::path::Path;
      pub fn git(&self, args: &[&str]) -> String;      // 실패 시 panic, stdout 반환
      pub fn commit(&self, subject: &str) -> String;   // 고유 파일 생성 후 커밋, 전체 해시 반환
      pub fn commit_file(&self, path: &str, content: &str, subject: &str) -> String;
      pub fn branch(&self, name: &str);                // 현재 커밋에서 생성(체크아웃 안 함)
      pub fn checkout(&self, name: &str);
      pub fn merge(&self, name: &str, no_ff: bool) -> String;
      pub fn tag(&self, name: &str);
      pub fn add_bare_remote(&self, name: &str) -> tempfile::TempDir; // bare 저장소 생성, remote 등록, 모든 ref push
  }
  ```
  커밋 시각은 호출마다 60초씩 증가하도록 `GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE`를 고정해 순서가 결정적이다.

- [ ] **Step 1: 실패하는 테스트** — `tests/smoke.rs`: `testrepo_commits_are_ordered`: `new()` 후 `commit("a")`, `commit("b")`; `git(&["rev-list","--count","HEAD"]).trim() == "2"`이고 `git(&["log","--format=%s"])`가 `"b\na\n"`.
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test smoke` → 컴파일 실패(크레이트 없음).
- [ ] **Step 3: 구현** — `cargo init --lib` 구조로 `Cargo.toml`(패키지명 `terminal-git-graph`, 라이브러리 `terminal_git_graph`, 바이너리 `tgg`, `license = "MIT"`, 위 Tech Stack 의존성), `LICENSE`(MIT, 저작권자는 사용자 git 설정의 이름), `deny.toml`(허용 라이선스 목록은 Global Constraints), `tests/support/mod.rs` 구현. `main.rs`는 빈 `fn main() {}`.
- [ ] **Step 4: 통과 확인** — `cargo test --test smoke` PASS, `cargo deny check licenses` 오류 없음.
- [ ] **Step 5: 커밋** — `git add -A && git commit -m "chore: scaffold crate, license check, test fixture"`

---

### Task 2: git 실행기와 저장소 판별

**Files:**
- Create: `src/git/mod.rs`, `src/repo.rs`, `tests/git_runner.rs`, `tests/repo_detect.rs`
- Modify: `src/lib.rs`(모듈 등록)

**Interfaces:**
- Produces:
  ```rust
  // src/git/mod.rs
  #[derive(Clone)] pub struct Git { /* dir, call counter */ }
  pub enum GitError { Failed { args: Vec<String>, code: Option<i32>, stdout: String, stderr: String }, Spawn(std::io::Error), InvalidName(String) }
  impl GitError { pub fn message(&self) -> String }  // stderr와 stdout을 합쳐 trim. 충돌 메시지는 stdout에 나온다.
  impl Git {
      pub fn at(dir: impl Into<PathBuf>) -> Git;
      pub fn dir(&self) -> &Path;
      pub fn run(&self, args: &[&str]) -> Result<String, GitError>;                       // stdout(UTF-8 lossy)
      pub fn run_with_env(&self, args: &[&str], env: &[(&str, &str)]) -> Result<String, GitError>;
      pub fn call_count(&self) -> usize;                                                   // 테스트용
  }
  pub fn validate_name(name: &str) -> Result<(), GitError>;   // 빈 문자열, '-' 시작, 공백/제어문자 포함 시 InvalidName

  // src/repo.rs
  pub enum RepoTarget { Repo { git: Git, bare: bool }, Folder(PathBuf) }
  pub fn detect(path: &Path) -> Result<RepoTarget, RepoError>;   // 경로가 없으면 RepoError::NotFound
  ```
  `Git`은 항상 `Command::new("git").args(..)`로 실행하고 셸을 거치지 않는다.

- [ ] **Step 1: 실패하는 테스트**
  - `git_runner.rs`: `run_returns_stdout`; `failed_command_exposes_stderr_and_code`(없는 브랜치 checkout → `Failed`, `message()`가 비어 있지 않음); `merge_conflict_message_comes_from_stdout`(충돌 merge의 `message()`에 `"CONFLICT"` 포함); `validate_name_rejects_dash_empty_and_whitespace`; `validate_name_accepts_feature_slash_names`; `call_count_increments`.
  - `repo_detect.rs`: `subdirectory_resolves_to_toplevel`; `linked_worktree_is_a_repo`; `bare_repo_dir_is_bare_true`; `plain_folder_is_folder`; `workspace_with_dot_bare_and_worktrees_is_folder`(이 프로젝트의 레이아웃); `missing_path_is_not_found`.
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test git_runner --test repo_detect` → FAIL(심볼 없음).
- [ ] **Step 3: 구현** — `Git::run_with_env`가 핵심이고 `run`은 빈 env로 위임. 판별은 `git rev-parse --show-toplevel`, `--is-bare-repository`로 한다(실패 시 `Folder`).
- [ ] **Step 4: 통과 확인** — 같은 명령 PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: git runner and repo detection"`

---

### Task 3: 커밋 로그와 ref 로딩

**Files:**
- Create: `src/git/log.rs`, `src/git/refs.rs`, `tests/log_refs.rs`

**Interfaces:**
- Consumes: `Git::run`.
- Produces:
  ```rust
  pub const PAGE_SIZE: usize = 300;                         // src/git/log.rs
  pub struct Commit { pub hash: String, pub parents: Vec<String>, pub author: String, pub email: String, pub timestamp: i64, pub subject: String }
  impl Commit { pub fn new(hash: &str, parents: &[&str], timestamp: i64, subject: &str) -> Commit }  // author/email 기본값 "", 테스트와 레인 계산용
  #[derive(Clone, Copy, PartialEq)] pub enum Scope { AllBranches, Current }
  pub struct LogQuery { pub scope: Scope, pub remotes: bool, pub skip: usize, pub limit: usize }
  pub fn load_commits(git: &Git, q: &LogQuery) -> Result<Vec<Commit>, GitError>;   // --date-order, 최신순
  pub fn count_commits(git: &Git, scope: Scope, remotes: bool) -> Result<usize, GitError>;

  // src/git/refs.rs
  pub enum RefKind { LocalBranch, RemoteBranch, Tag }
  pub struct RefLabel { pub name: String, pub kind: RefKind, pub target: String, pub remotes: Vec<String> }  // LocalBranch의 remotes: 같은 이름·같은 커밋을 가리키는 원격 이름들(`main | origin`)
  pub enum Head { Branch(String), Detached(String), Unborn(String) }   // Detached는 해시, Unborn은 브랜치 이름
  pub struct Refs { pub labels: Vec<RefLabel>, pub head: Head }
  impl Refs { pub fn labels_for(&self, hash: &str) -> Vec<&RefLabel> }
  pub fn load_refs(git: &Git) -> Result<Refs, GitError>;
  ```
  범위 의미: AllBranches = `--branches --tags` (+`remotes==true`이면 `--remotes`), Current = `HEAD`. 필드 구분은 `%x1f`와 `-z`로 한다(제목에 탭/따옴표/유니코드가 있어도 안전).

- [ ] **Step 1: 실패하는 테스트** (`tests/log_refs.rs`)
  `loads_commits_newest_first_with_parents`; `paging_with_skip_and_limit`(25커밋, limit 10 skip 10이 11~20번째); `count_matches_rev_list`; `current_scope_excludes_other_branches`; `remotes_off_hides_commits_only_reachable_from_remotes`; `subject_with_tab_quote_and_unicode_roundtrips`; `tags_are_loaded_as_labels`; `local_branch_folds_matching_remote`(`labels[main].remotes == ["origin"]`, 원격 브랜치가 별도 라벨로 중복되지 않음); `remote_only_branch_has_own_label`; **`empty_repo_has_no_commits_and_unborn_head`**(`load_commits`는 `Ok(vec![])`, `Head::Unborn("main")`); **`detached_head_is_reported`**(`Head::Detached(hash)`).
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test log_refs` → FAIL.
- [ ] **Step 3: 구현** — 로그는 `git log --date-order --format=...`; ref는 `git for-each-ref`와 `git symbolic-ref -q HEAD`/`rev-parse`. 커밋이 하나도 없을 때 `git log`의 오류는 빈 결과로 바꾼다.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: commit log and ref loading"`

---

### Task 4: 레인 계산 (순수 로직)

**Files:**
- Create: `src/graph/mod.rs`, `src/graph/lanes.rs`, `tests/lanes.rs`

**Interfaces:**
- Consumes: `Commit::new`.
- Produces:
  ```rust
  #[derive(Clone, Copy, PartialEq, Debug)] pub struct LaneSeg { pub lane: usize, pub color: usize }
  pub struct GraphRow {
      pub hash: String, pub lane: usize, pub color: usize,
      pub pass: Vec<LaneSeg>,    // 이 행을 수직으로 통과하는 레인(노드와 무관)
      pub joins: Vec<LaneSeg>,   // 위에서 내려와 이 노드로 합류하는 레인(자기 레인 제외)
      pub forks: Vec<LaneSeg>,   // 이 노드에서 아래로 갈라져 나가는 레인(2번째 이후 부모)
      pub continues: bool,       // 자기 레인이 첫 부모로 아래에 이어지는가(루트면 false)
      pub is_merge: bool,        // 부모가 2개 이상
      pub width: usize,          // 이 행이 차지하는 레인 수
  }
  pub const PALETTE_LEN: usize = 8;
  pub struct LaneBuilder { /* 활성 레인이 기다리는 해시 목록 */ }
  impl LaneBuilder { pub fn new() -> Self; pub fn push(&mut self, c: &Commit) -> GraphRow; }
  ```
  `push`는 날짜순(자식이 부모보다 먼저)으로 호출된다. 상태가 호출 사이에 유지되어 페이지 단위로 이어서 넣을 수 있다. 새 레인은 `color = 다음 번호 % PALETTE_LEN`을 받고, 한 레인의 색은 합류 전까지 유지된다. 합류로 비어진 레인은 가장 왼쪽 빈 칸부터 재사용한다.

- [ ] **Step 1: 실패하는 테스트** (`tests/lanes.rs`, 합성 `Commit::new`만 사용)
  `linear_history_stays_in_lane_0`; `fork_creates_second_lane_and_join_closes_it`(A←B, A←C 후 A에서 `joins.len()==1`); `merge_commit_forks_to_second_parent`(`is_merge`, `forks.len()==1`); `closed_lane_is_reused_by_next_branch`; **`octopus_merge_with_three_parents`**(`forks.len()==2`, 이어지는 행들의 `pass`와 `joins`가 겹치지 않음); `lane_color_is_stable_until_join`; `incremental_push_equals_single_batch`(같은 목록을 한 번에/두 번에 나눠 넣어 행이 동일); `root_commit_does_not_continue`.
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test lanes` → FAIL.
- [ ] **Step 3: 구현** — `Vec<Option<(String, usize)>>`(레인별로 기다리는 해시와 색) 모델. 노드가 속할 레인은 자신을 기다리는 가장 왼쪽 레인이고, 그 외에 자신을 기다리는 레인은 `joins`로 닫는다. 부모마다 이미 그 부모를 기다리는 레인이 있으면 그리로 합류시키고(`forks`에 기록), 없으면 새 레인을 연다.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: incremental lane assignment"`

---

### Task 5: 그래프 글리프 렌더 (순수 로직)

**Files:**
- Create: `src/graph/render.rs`, `tests/graph_render.rs`

**Interfaces:**
- Consumes: `GraphRow`, `LaneSeg`.
- Produces:
  ```rust
  #[derive(Clone, PartialEq, Debug)] pub struct Cell { pub ch: char, pub color: Option<usize>, pub dim: bool }
  pub struct RenderOpts { pub is_head: bool, pub ascii: bool }
  pub fn render_row(row: &GraphRow, opts: &RenderOpts) -> Vec<Cell>;     // 길이 == 2 * row.width, 레인당 2칸
  pub fn render_uncommitted(head_lane: usize, width: usize, ascii: bool) -> Vec<Cell>;   // 점선 연결 행
  ```
  글리프: 노드 `●`, HEAD `◯`, 수직 `│`, 수평 `─`, 곡선 `╭ ╮ ╰ ╯`, 점선 `┆`. ASCII 대체: `*`, `O`, `|`, `-`, `/`, `\`, `:`. 병합 커밋(`is_merge`)의 모든 셀은 `dim = true`.

- [ ] **Step 1: 실패하는 테스트** (`tests/graph_render.rs`; 셀을 문자열로 이어 비교)
  `linear_row_is_node_and_blank`; `merge_row_draws_curve_to_second_parent`(예: `"●─╮"` 형태); `join_row_draws_curve_from_right`; `head_row_uses_ring_glyph`; `merge_cells_are_dim`; `pass_through_lane_draws_vertical_bar`; `uncommitted_row_uses_dashed_connector`; `ascii_mode_emits_only_ascii`.
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test graph_render` → FAIL.
- [ ] **Step 3: 구현** — 레인 `i`는 칸 `2i`(노드/수직)와 `2i+1`(수평 연결)을 쓴다. 수평선은 노드 레인과 `joins`/`forks` 레인 사이를 채운다.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: render graph rows to glyph cells"`

---

### Task 6: 작업 트리 상태 (Uncommitted 행 데이터)

**Files:**
- Create: `src/git/status.rs`, `tests/status.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct WorkStatus { pub staged: usize, pub unstaged: usize, pub untracked: usize, pub branch: Option<String>, pub has_upstream: bool, pub ahead: usize, pub behind: usize }
  impl WorkStatus { pub fn changed_total(&self) -> usize; pub fn is_clean(&self) -> bool }   // total = 파일 수(스테이지+언스테이지에 모두 있는 파일은 1개), untracked 포함
  pub fn parse_status_v2(out: &str) -> WorkStatus;       // `git status --porcelain=v2 --branch` 출력 파싱
  pub fn load_status(git: &Git) -> Result<WorkStatus, GitError>;
  ```
- [ ] **Step 1: 실패하는 테스트** (`tests/status.rs`): `parses_branch_ahead_behind`(`# branch.ab +2 -1`); `no_upstream_is_flagged`; `counts_staged_unstaged_untracked`(문자열 샘플); `renamed_entry_counts_once`; `file_in_both_index_and_worktree_counts_once`; `clean_repo_is_clean`; `load_status_on_real_repo`(파일 수정 + 새 파일 후 `changed_total()==2`).
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test status` → FAIL.
- [ ] **Step 3: 구현** — porcelain v2의 `1`/`2`(변경), `u`(충돌), `?`(untracked) 행을 분류. XY 코드의 X≠`.`이면 staged, Y≠`.`이면 unstaged.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: working tree status parsing"`

---

### Task 7: 커밋 상세, diff, 상세 캐시

**Files:**
- Create: `src/git/detail.rs`, `tests/detail.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct FileChange { pub path: String, pub status: char, pub added: Option<u32>, pub deleted: Option<u32> }   // 바이너리면 None
  pub struct CommitDetail { pub hash: String, pub parents: Vec<String>, pub author: String, pub email: String, pub timestamp: i64, pub message: String, pub files: Vec<FileChange> }
  pub fn load_detail(git: &Git, hash: &str) -> Result<CommitDetail, GitError>;           // 병합 커밋은 첫 부모 기준 diff
  pub fn load_file_diff(git: &Git, hash: &str, path: &str) -> Result<String, GitError>;
  pub fn load_worktree_changes(git: &Git) -> Result<Vec<FileChange>, GitError>;          // staged+unstaged+untracked
  pub fn load_worktree_diff(git: &Git, change: &FileChange) -> Result<String, GitError>; // untracked는 파일 내용을 읽어 반환
  pub const MAX_UNTRACKED_BYTES: usize = 1_048_576;                                       // 초과분은 잘라내고 "[truncated]" 덧붙임
  pub struct DetailCache { /* 최대 256개 LRU */ }
  impl DetailCache { pub fn new() -> Self; pub fn get_or_load(&mut self, git: &Git, hash: &str) -> Result<&CommitDetail, GitError> }
  ```
- [ ] **Step 1: 실패하는 테스트** (`tests/detail.rs`): `detail_lists_files_with_numstat`; `merge_commit_diffs_against_first_parent`(병합으로 들어온 파일이 목록에 있음); `binary_file_counts_are_none`; `file_diff_is_unified_diff`(`@@` 포함); `worktree_changes_include_untracked_files`; `untracked_content_is_returned`; `untracked_content_truncated_at_limit`(2 MiB 파일 → 길이 ≤ 한도 + 마커); `detail_cache_hit_makes_no_second_git_call`(`call_count` 불변).
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test detail` → FAIL.
- [ ] **Step 3: 구현** — 메시지/메타는 `git show -s --format=...`, 파일은 `git diff-tree -m --first-parent --numstat --name-status -r`; 루트 커밋은 `--root`.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: commit detail, diff and cache"`

---

### Task 8: TUI 골격과 그래프 표

**Files:**
- Create: `src/app.rs`, `src/terminal.rs`, `src/ui/mod.rs`, `src/ui/theme.rs`, `src/ui/layout.rs`, `src/ui/table.rs`, `src/ui/header.rs`, `tests/ui_table.rs`
- Modify: `src/main.rs`(현재 디렉터리의 저장소 그래프를 여는 최소 연결; 인자 처리는 Task 16)

**Interfaces:**
- Consumes: Task 3, 4, 5, 6의 로딩/계산/렌더 함수.
- Produces:
  ```rust
  pub enum ColorDepth { TrueColor, Indexed256 }
  impl ColorDepth { pub fn detect(colorterm: Option<&str>, term: Option<&str>) -> Self }   // COLORTERM=truecolor|24bit → TrueColor
  pub struct Columns { pub author: bool, pub hash: bool }
  pub const AUTHOR_MIN_WIDTH: u16 = 70; pub const HASH_MIN_WIDTH: u16 = 100;   // 계획상의 판단, 조정 가능
  pub fn columns_for(width: u16) -> Columns;
  pub fn truncate_to_width(s: &str, width: usize) -> String;     // 표시 폭 기준, 잘리면 '…'
  pub fn relative_time(now: i64, ts: i64) -> String;             // 1분 미만 "now", "5m", "3h", "10d", "2mo", "1y"
  pub enum Command { Quit, Up, Down, PageUp, PageDown, Top, Bottom, SelectRow(usize), ToggleScope, ToggleRemotes, Refresh }
  pub struct App { /* git, rows, refs, selection, scope, remotes, 로드 상태 */ }
  impl App {
      pub fn new(git: Git, depth: ColorDepth) -> Result<App, GitError>;
      pub fn apply(&mut self, cmd: Command) -> Result<(), GitError>;     // Bottom/PageDown이 끝에 닿으면 다음 PAGE_SIZE개를 로드
      pub fn draw(&self, frame: &mut ratatui::Frame);
      pub fn selected_hash(&self) -> Option<&str>;
  }
  ```
  행 구성: 맨 위 "Uncommitted changes (N)" 행(작업 트리가 깨끗하지 않고 bare가 아닐 때), 이후 커밋 행: `[그래프][라벨 배지][제목][날짜][작성자][해시]`. 체크아웃 브랜치 배지에 `✓`, HEAD 행은 링과 색조 배경, 헤더에 브랜치/로드한 커밋 수/툴바(All·Current, Remotes). 전체 커밋 수는 `count_commits`가 빠르면 사용한다.

- [ ] **Step 1: 실패하는 테스트** (`tests/ui_table.rs`, `TestBackend` 버퍼의 문자열로 검증)
  `renders_rows_with_labels_and_head_check`(라벨 `✓main`, 제목, 상대 시간); `narrow_width_hides_author_and_hash`(폭 50); `wide_width_shows_author_and_hash`(폭 120); **`cjk_and_emoji_subject_truncated_by_display_width`**(한글/이모지 제목이 열 경계를 넘지 않음, `truncate_to_width("한글한글", 5) == "한글…"`); **`tiny_terminal_shows_too_small_message_without_panic`**(10×3 및 리사이즈 20→10); **`empty_repo_shows_no_commits_message`**("No commits yet"); `scrolling_to_end_loads_next_page`(350커밋 저장소에서 `Bottom` 후 행 수 350); `scope_and_remotes_toggles_reload`; `uncommitted_row_is_first_with_count`; `bare_repo_has_no_uncommitted_row`; `color_depth_detect_table`; `relative_time_table`(59s→"now", 3h, 10d 등).
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test ui_table` → FAIL.
- [ ] **Step 3: 구현** — 터미널 진입/복원은 `src/terminal.rs`(raw mode, alternate screen, 마우스 캡처, 패닉 훅에서 복원). `table.rs`는 열 폭을 먼저 계산하고 설명 열이 남은 폭을 쓴다. 라벨이 너무 많으면 잘린다. 색 팔레트는 `theme.rs`(`PALETTE_LEN`개, `Indexed256`이면 가장 가까운 인덱스로 변환).
- [ ] **Step 4: 통과 확인** — PASS. 수동: `cargo run`을 이 저장소에서 실행해 그래프가 보이고 `q`로 종료되는지.
- [ ] **Step 5: 커밋** — `git commit -m "feat: TUI shell and graph table"`

---

### Task 9: 커밋 상세 패널과 diff 보기

**Files:**
- Create: `src/ui/detail.rs`, `src/ui/diff.rs`, `tests/ui_detail.rs`
- Modify: `src/app.rs`(상세/ diff 상태), `src/ui/mod.rs`

**Interfaces:**
- Consumes: Task 7 `DetailCache`, `load_file_diff`, `load_worktree_*`.
- Produces: `Command::{OpenDetail, CloseDetail, FocusFiles, FileUp, FileDown, OpenDiff, CloseDiff, DiffScrollUp, DiffScrollDown}`를 `Command`에 추가; `pub const DETAIL_SPLIT_MIN_HEIGHT: u16 = 30;` (이 높이 이상이면 표 아래에 분할, 미만이면 상세가 전체 화면으로 전환; 계획상의 판단).
- [ ] **Step 1: 실패하는 테스트** (`tests/ui_detail.rs`): `selecting_commit_shows_message_hash_parents_author_date`; `file_list_shows_status_and_plus_minus`; `short_terminal_switches_to_fullscreen_detail`; `open_diff_shows_hunk_and_scrolls`; `esc_returns_from_diff_to_detail`; `uncommitted_row_detail_lists_changed_files`; `untracked_file_diff_shows_content`; `merge_commit_detail_shows_both_parents`.
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test ui_detail` → FAIL.
- [ ] **Step 3: 구현** — diff 줄은 `+`는 초록, `-`는 빨강, `@@`는 강조. 상세 로딩이 늦으면 헤더(메시지/해시)를 먼저 그리고 파일 목록은 "Loading…"으로 둔다.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: commit detail panel and diff view"`

---

### Task 10: 실시간 갱신

**Files:**
- Create: `src/watch.rs`, `tests/watch.rs`
- Modify: `src/app.rs`, `src/terminal.rs`(이벤트 루프에서 감시 이벤트 수신)

**Interfaces:**
- Produces:
  ```rust
  pub enum WatchEvent { RefsChanged, WorktreeChanged }
  pub struct WatchHandle { /* 드롭 시 감시 중지 */ }
  pub const DEBOUNCE_MS: u64 = 200;
  pub fn watch(git: &Git, tx: std::sync::mpsc::Sender<WatchEvent>) -> notify::Result<WatchHandle>;
  // App
  impl App { pub fn on_watch(&mut self, ev: WatchEvent) -> Result<(), GitError>; }   // RefsChanged: 첫 페이지 다시 로드하되 선택 커밋 유지, WorktreeChanged: 상태만 갱신
  ```
  감시 대상: `git rev-parse --git-common-dir`의 `HEAD`, `refs/`, `packed-refs`와 작업 트리(`.git` 내부 변경은 위 목록 외 무시).
- [ ] **Step 1: 실패하는 테스트** (`tests/watch.rs`, 이벤트는 최대 3초 대기): `commit_from_another_process_emits_refs_changed`; `checkout_emits_refs_changed`; `edit_file_emits_worktree_changed`; `index_lock_noise_is_ignored`(.git 내부 임의 파일 변경은 이벤트 없음); `burst_of_changes_is_debounced`(연속 10회 수정이 이벤트 3개 이하); `reload_keeps_selected_commit`(`App::on_watch` 후 `selected_hash` 동일); `reload_after_new_commit_shows_it_on_top`.
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test watch` → FAIL.
- [ ] **Step 3: 구현** — notify의 권장 watcher + 자체 디바운스. 이벤트 루프는 입력과 감시 채널을 함께 폴링한다.
- [ ] **Step 4: 통과 확인** — PASS. 수동: 다른 터미널에서 커밋하면 그래프가 즉시 바뀌는지.
- [ ] **Step 5: 커밋** — `git commit -m "feat: live refresh on ref and worktree changes"`

---

### Task 11: 입력 체계(키/마우스), 메뉴·대화상자 틀

**Files:**
- Create: `src/input.rs`, `src/ui/menu.rs`, `src/ui/dialog.rs`, `tests/input.rs`
- Modify: `src/app.rs`, `src/ui/table.rs`(라벨 히트 영역 노출)

**Interfaces:**
- Produces:
  ```rust
  pub fn translate_key(key: crossterm::event::KeyEvent, mode: InputMode) -> Option<Command>;
  pub enum InputMode { Graph, Detail, Diff, Menu, Dialog }
  pub const DOUBLE_CLICK_MS: u64 = 400;
  pub enum ClickKind { Single, Double }
  pub struct ClickTracker; impl ClickTracker { pub fn new() -> Self; pub fn click(&mut self, pos: (u16, u16), now: std::time::Instant) -> ClickKind }
  pub enum Hit { Row(usize), Label { row: usize, index: usize }, File(usize), Toolbar(ToolbarButton) }
  pub enum ToolbarButton { Scope, Remotes, Fetch }
  pub fn hit_test(app: &App, area: ratatui::layout::Rect, pos: (u16, u16)) -> Option<Hit>;
  pub struct MenuItem { pub label: String, pub enabled: bool, pub danger: bool, pub id: MenuId }   // MenuId는 Task 13에서 Action과 연결
  pub enum Dialog { Confirm { title: String, body: String, danger: bool }, Input { title: String, prompt: String, value: String }, Error { title: String, message: String } }
  ```
  키: `j/k/↑/↓` 이동, `g/G` 처음/끝, `PgUp/PgDn`, `Enter` 상세/활성화, `Esc` 뒤로, `m` 선택 대상 메뉴(우클릭과 동일), `a` All↔Current, `r` Remotes, `f` Fetch, `?` 도움말, `q` 종료. 이 키 매핑은 계획상의 판단이다.
- [ ] **Step 1: 실패하는 테스트** (`tests/input.rs`): `key_table_maps_to_commands`; `single_then_double_click_within_window`; `clicks_at_different_positions_are_not_double`; `click_on_row_selects_it`; `click_on_label_targets_that_label`(여러 라벨 중 두 번째); `right_click_opens_menu_at_position`; `menu_arrows_skip_disabled_items_and_enter_confirms`; `confirm_dialog_enter_accepts_esc_cancels`; `input_dialog_rejects_value_starting_with_dash`; `error_dialog_shows_message_verbatim`; `scroll_wheel_moves_selection`.
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test input` → FAIL.
- [ ] **Step 3: 구현** — 메뉴는 화면 밖으로 나가면 위치를 보정한다. 대화상자는 모달이며 열려 있는 동안 다른 입력은 무시된다.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: key/mouse input, menu and dialog primitives"`

---

### Task 12: Git 동작 실행기 (UI 없음)

**Files:**
- Create: `src/actions.rs`, `tests/actions.rs`

**Interfaces:**
- Consumes: `Git::run`, `validate_name`, `GitError::message`.
- Produces:
  ```rust
  pub enum ResetMode { Soft, Mixed, Hard }
  pub enum Action {
      CheckoutBranch(String),
      CheckoutRemote { remote: String, branch: String, local_name: String },
      CheckoutCommit(String),
      RenameBranch { old: String, new: String },
      DeleteBranch { name: String, force: bool },
      DeleteRemoteBranch { remote: String, name: String },
      MergeInto(String),                 // 대상 ref를 현재 브랜치로 merge
      RebaseOnto(String),                // 현재 브랜치를 대상 위로 rebase
      PushBranch { remote: String, name: String },
      AddTag { hash: String, name: String, message: Option<String> },
      DeleteTag(String),
      PushTag { remote: String, name: String },
      CreateBranch { hash: String, name: String },
      CherryPick(String), Revert(String),
      Reset { hash: String, mode: ResetMode },
      Commit { message: String, stage_all: bool },
      Stash,                              // tracked 변경만 (`git stash push`)
      ResetWorktree,                      // `git reset --hard HEAD` (tracked만)
      Fetch, FetchAll,                    // FetchAll은 GIT_TERMINAL_PROMPT=0
      Pull,                               // `git pull --ff-only`
      PushCurrent,
  }
  pub fn needs_confirm(a: &Action) -> bool;                       // CheckoutBranch와 Fetch만 false
  pub fn execute(git: &Git, a: &Action) -> Result<String, GitError>;
  ```
  Push 대상 remote가 없으면 브랜치의 upstream remote, 그것도 없고 remote가 하나뿐이면 그 remote를 쓴다. 이름 인자는 모두 실행 전에 `validate_name`을 거친다.
- [ ] **Step 1: 실패하는 테스트** (`tests/actions.rs`, 실제 임시 저장소와 `add_bare_remote`): `needs_confirm_table`; 동작별 `checkout_local_branch`, `checkout_remote_creates_tracking_branch`, `checkout_commit_detaches_head`, `rename_branch`, `delete_branch_refuses_unmerged_without_force`, `delete_remote_branch`, `merge_into_current`, `rebase_onto`, `push_branch_updates_remote`, `add_annotated_and_lightweight_tag`, `delete_and_push_tag`, `create_branch_at_commit`, `cherry_pick`, `revert`, `reset_soft_mixed_hard`(HEAD와 인덱스/작업 트리 상태 각각 검증), `commit_stage_all_includes_untracked`, `commit_staged_only_leaves_other_changes`, `stash_removes_tracked_changes_and_keeps_untracked`, **`reset_worktree_reverts_tracked_and_keeps_untracked`**, `fetch_brings_new_remote_commits`, `pull_ff_only`, `names_starting_with_dash_are_rejected_before_running_git`(`call_count` 불변), **`merge_conflict_returns_error_with_conflict_text`**(`Err`, `message()`에 "CONFLICT").
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test actions` → FAIL.
- [ ] **Step 3: 구현** — 각 `Action`을 한 번의 git 호출(또는 `add -A` + `commit`처럼 순차 호출)로 매핑한다. 변경 동작의 추가 `--force` 같은 플래그는 사용하지 않는다(`DeleteBranch.force`만 `-D`).
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: git action executor"`

---

### Task 13: 컨텍스트 메뉴, 확인 대화상자, 작업 스레드, 클립보드 연결

**Files:**
- Create: `src/menus.rs`, `src/jobs.rs`, `src/clipboard.rs`, `tests/menus.rs`, `tests/clipboard.rs`
- Modify: `src/app.rs`, `src/ui/menu.rs`, `src/ui/dialog.rs`

**Interfaces:**
- Consumes: Task 11 `MenuItem`/`Dialog`, Task 12 `Action`/`execute`/`needs_confirm`.
- Produces:
  ```rust
  pub enum Target { LocalBranch(String), RemoteBranch { remote: String, name: String }, Tag(String), Commit(String), Uncommitted }
  pub struct MenuCtx { pub head: Head, pub has_staged: bool, pub remotes: Vec<String> }
  pub fn menu_for(t: &Target, ctx: &MenuCtx) -> Vec<MenuItem>;     // MenuId는 Action 또는 Copy(String)로 해석
  pub enum MenuId { Do(Action), Copy(String) }                     // Task 11의 MenuId를 이 정의로 대체
  // src/jobs.rs
  pub struct JobRunner; impl JobRunner { pub fn new() -> Self; pub fn spawn(&self, git: Git, a: Action) -> std::sync::mpsc::Receiver<Result<String, GitError>> }
  // src/clipboard.rs
  pub fn osc52_sequence(text: &str) -> String;                     // "\x1b]52;c;<base64>\x07"
  pub enum CopyVia { Pbcopy, Osc52 }
  pub fn copy(text: &str) -> Result<CopyVia, std::io::Error>;      // macOS는 pbcopy, 아니면 OSC 52
  ```
  메뉴 항목(스펙 4.4 그대로): 로컬 브랜치 = Checkout, Rename, Delete, Merge into current, Rebase onto current, Push, Copy name. 원격 브랜치 = Checkout, Merge, Delete on remote, Copy name. 태그 = Delete, Push, Copy name. 커밋 = Add tag, Create branch, Checkout, Cherry pick, Revert, Merge, Rebase, Reset(Soft/Mixed/Hard), Copy hash, Copy subject. Uncommitted = Commit (stage all), Commit (staged only), Stash, Reset. 현재 브랜치 자신에 대한 Delete/Merge/Rebase는 비활성; detached HEAD면 Merge/Rebase/Push 및 현재 브랜치가 필요한 항목은 비활성; staged가 없으면 "Commit (staged only)" 비활성. 작업은 `JobRunner`로 별도 스레드에서 돌고 UI는 "Running…"을 표시하며 계속 반응한다. 성공하면 즉시 리로드, 실패하면 `Dialog::Error`에 `GitError::message()`를 그대로 넣는다.
- [ ] **Step 1: 실패하는 테스트**
  - `menus.rs`: 대상별 `menu_for` 라벨 목록이 위 스펙과 정확히 일치; `current_branch_delete_merge_rebase_disabled`; **`detached_head_disables_current_branch_items`**; `commit_staged_only_disabled_without_staged`; `reset_items_are_danger`; `remote_checkout_without_local_opens_input_dialog_with_default_name`; `confirm_dialog_body_for_reset_worktree_mentions_untracked_kept`("Untracked files are kept."); `failed_action_shows_error_dialog_with_git_message`; `local_checkout_and_fetch_run_without_dialog`; `job_runner_delivers_result_without_blocking`.
  - `clipboard.rs`: `osc52_sequence_for_hi`(`"\x1b]52;c;aGk=\x07"`); `copy_falls_back_to_dialog_value_when_all_fail`는 UI 쪽에서 `Dialog::Error`가 값을 포함하는지로 검증.
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test menus --test clipboard` → FAIL.
- [ ] **Step 3: 구현** — 더블클릭 라벨(로컬 브랜치)은 `Do(CheckoutBranch)`, 원격 전용 라벨은 로컬 이름을 묻는 입력 대화상자. 체크아웃 후 `Refs`/`status`를 다시 읽는다.
- [ ] **Step 4: 통과 확인** — PASS. 수동: 임시 저장소에서 우클릭 → 동작 → 확인 흐름을 마우스와 키보드로 모두 시도.
- [ ] **Step 5: 커밋** — `git commit -m "feat: context menus, confirm dialogs, jobs, clipboard"`

---

### Task 14: Manygit — 저장소 스캔과 상태

**Files:**
- Create: `src/manygit/mod.rs`, `src/manygit/scan.rs`, `src/manygit/status.rs`, `tests/manygit_scan.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct RepoEntry { pub path: PathBuf, pub name: String, pub group: PathBuf }   // group = 부모 폴더
  pub enum ScanError { RootIsHomeOrAncestor, Io(std::io::Error) }
  pub fn scan(root: &Path, depth: u8, home: &Path) -> Result<Vec<RepoEntry>, ScanError>;
  pub enum SyncState { InSync, Ahead(usize), Behind(usize), Diverged(usize, usize), NotPushed, NoRemote }   // NotPushed: remote는 있으나 upstream 없음
  pub struct RepoStatus { pub branch: Option<String>, pub dirty: bool, pub sync: SyncState, pub latest_tag: Option<String> }
  pub fn repo_status(path: &Path, with_tag: bool) -> RepoStatus;
  ```
  저장소 판별은 `.git`(디렉터리 또는 파일) 존재. 저장소 안으로는 더 들어가지 않는다. 숨김 폴더와 `node_modules`/`build`/`dist`/`vendor`/`target`은 건너뜀. 스캔은 Rust 표준 라이브러리 파일 순회로 한다(외부 `find` 불필요).
- [ ] **Step 1: 실패하는 테스트** (`tests/manygit_scan.rs`): `finds_nested_repos_up_to_depth`(깊이 1~5 각각); `does_not_descend_into_repos`; `skips_hidden_and_build_folders`; `refuses_home_and_its_ancestors`; `linked_worktree_dot_git_file_is_detected`; `entries_grouped_by_parent_folder`; `status_ahead_behind_diverged`; `status_not_pushed_when_no_upstream`; `status_no_remote`; `status_dirty_includes_untracked`; `latest_tag_only_when_requested`.
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test manygit_scan` → FAIL.
- [ ] **Step 3: 구현** — 상태는 `git status --porcelain=v2 --branch`와 `git remote`, 태그는 `git describe --tags --abbrev=0`.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: manygit scan and status"`

---

### Task 15: Manygit 화면, 설정, 원격 페이지 열기

**Files:**
- Create: `src/manygit/view.rs`, `src/config.rs`, `src/remote_url.rs`, `tests/manygit_ui.rs`, `tests/config.rs`, `tests/remote_url.rs`
- Modify: `src/app.rs`(목록 모드 ↔ 그래프 모드 전환), `src/menus.rs`

**Interfaces:**
- Consumes: Task 14, Task 13 메뉴 틀, Task 12 `Action::{Fetch,FetchAll,Pull,PushCurrent}`.
- Produces:
  ```rust
  pub struct Config { pub scan_depth: u8, pub show_tags: bool, pub theme: String }      // 기본 3, false, "default"
  impl Config { pub fn load(path: &Path) -> (Config, Option<String>) }                  // 깊이는 1..=5로 보정, 파싱 실패 시 기본값 + 경고 문자열
  pub fn default_config_path() -> PathBuf;                                              // ~/.config/terminal-git-graph/config.toml
  pub fn to_https(remote: &str) -> Option<String>;                                      // 원격 페이지 URL
  pub const MANYGIT_REFRESH_SECS: u64 = 15;
  pub fn should_refresh(last: std::time::Instant, now: std::time::Instant) -> bool;
  pub fn repo_menu(status: &RepoStatus, remotes: &[String]) -> Vec<MenuItem>;           // 첫 동작: Behind→Pull, Ahead→Push, 그 외 Fetch
  ```
  저장소 메뉴: Pull/Push/Fetch(↑/↓가 요구하는 것이 맨 앞), Commit, Stash, Reset, Open remote page, Copy path. 헤더의 저장소 이름에서 다른 저장소로 전환하는 드롭다운과 "‹ Manygit"로 목록 복귀. `F`(대문자)로 Fetch all(원격이 있는 모든 저장소, `GIT_TERMINAL_PROMPT=0`).
- [ ] **Step 1: 실패하는 테스트**
  - `remote_url.rs`: `scp_style_git_at_host`(`git@github.com:o/r.git` → `https://github.com/o/r`); `ssh_scheme`; `https_with_dot_git`; `https_with_credentials_are_stripped`; `non_url_returns_none`.
  - `config.rs`: `missing_file_gives_defaults`; `depth_is_clamped_to_1_5`; `invalid_toml_gives_defaults_and_warning`.
  - `manygit_ui.rs`: `list_shows_groups_and_status_markers`(↑ ↓ 미푸시 원격없음); `enter_opens_graph_and_back_returns_to_list`; `header_name_dropdown_switches_repo`; `first_menu_action_follows_ahead_behind`; `fetch_all_skips_repos_without_remote`; `should_refresh_after_15_seconds_only`; `visited_repo_graph_is_cached_for_instant_reopen`(재진입 시 `call_count` 증가 없이 첫 화면 그림).
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test remote_url --test config --test manygit_ui` → FAIL.
- [ ] **Step 3: 구현** — 열기는 macOS `open`, 그 외 `xdg-open`(실패하면 URL을 대화상자에 표시). 방문한 저장소의 `App` 상태(행, 선택, 캐시)를 경로를 키로 보관한다.
- [ ] **Step 4: 통과 확인** — PASS. 수동: 여러 저장소가 든 폴더에서 목록 → 그래프 → 복귀.
- [ ] **Step 5: 커밋** — `git commit -m "feat: manygit view, config, remote page"`

---

### Task 16: CLI 진입점과 최종 연결

**Files:**
- Modify: `src/main.rs`, `src/lib.rs`
- Create: `tests/cli.rs`

**Interfaces:**
- Consumes: `repo::detect`, `Config::load`, `App`, Manygit 모드.
- Produces: `tgg [PATH]`(기본 `.`), `tgg --version`, `tgg --help`. `Repo`면 그래프(bare면 Uncommitted 행과 작업 트리 동작 없이), `Folder`면 Manygit 목록. 경로가 없으면 메시지와 함께 종료 코드 2. 설정 경고는 시작 직후 한 번 표시한다.
- [ ] **Step 1: 실패하는 테스트** (`tests/cli.rs`, `env!("CARGO_BIN_EXE_tgg")`로 실행): `version_flag_prints_package_version`; `missing_path_exits_2_with_message`; `help_mentions_path_argument`; `non_tty_stdout_exits_with_error_instead_of_hanging`(표준 출력이 터미널이 아니면 안내 메시지와 종료 코드 1).
- [ ] **Step 2: 실행해 실패 확인** — `cargo test --test cli` → FAIL.
- [ ] **Step 3: 구현** — clap derive. `detect` 결과에 따라 `App::new` 또는 Manygit 모드로 시작하고 이벤트 루프(입력 + 감시 + 작업 결과 + 15초 타이머)를 돌린다.
- [ ] **Step 4: 통과 확인** — PASS. 수동 스모크: 이 저장소 워크스페이스 루트(`..`)에서 `tgg`(Manygit 목록에 worktree들), `develop`에서 `tgg`(그래프), `.bare`에서 `tgg`(bare 그래프).
- [ ] **Step 5: 커밋** — `git commit -m "feat: CLI entry and mode routing"`

---

### Task 17: 성능 검증, README, 마무리

**Files:**
- Create: `tests/perf.rs`, `tests/support/big_repo.rs`, `docs/keys.md`
- Modify: `README.md`

**Interfaces:**
- Produces: `support::big_repo::generate(commits: usize) -> TestRepo` (`git fast-import`로 병합 포함 합성 히스토리 생성).
- [ ] **Step 1: 실패하는 테스트** (`tests/perf.rs`, 모두 `#[ignore]`, `cargo test --release -- --ignored`로 실행): `first_screen_under_one_second_on_50k_commits`(`load_refs` + `load_commits(300)` + `load_status` + 첫 `draw`의 합이 1초 미만, 캐시 없는 콜드 스타트); `next_page_under_300ms`; `lane_building_50k_rows_under_one_second`.
- [ ] **Step 2: 실행해 결과 확인** — 기준을 못 넘으면 병목(로그 호출, 레인 계산, 렌더)을 측정해 고친다. 저장소를 수정하는 방법(commit-graph 쓰기 등)은 사용하지 않는다.
- [ ] **Step 3: 문서** — `README.md`에 소개, 설치(`cargo install --path .`), 사용법, 기능 목록, 설정 파일, 라이선스(MIT), 출처 표기("영감: herdr-git-graph, VS Code Git Graph; 코드는 공유하지 않음"). `docs/keys.md`에 키 매핑. 자체 스크린샷은 ghostty에서 직접 캡처해 `docs/`에 추가한다.
- [ ] **Step 4: 전체 검증** — `cargo test`, `cargo test --release -- --ignored`, `cargo clippy -- -D warnings`, `cargo deny check licenses` 모두 통과.
- [ ] **Step 5: 커밋** — `git commit -m "docs: README and perf checks"`

---

## Self-Review 메모

- **스펙 커버리지**: 4.1 → Task 3·4·5·6·8, 4.2 → Task 7·9, 4.3 → Task 10(+Task 7 캐시, Task 15 저장소 캐시), 4.4 → Task 12·13, 4.5 → Task 14·15, 4.6 → Task 11·13, 제약/라이선스 → Task 1·17, 성공 기준 → Task 17. 누락 없음. 비목표(검색, 드롭다운 선택, 그래프 내 pull, interactive rebase, 파일별 동작)는 의도적으로 제외했다.
- **계획상의 판단(스펙에 없던 선택, 구현 전 확인 필요)**: `Unicode-3.0`/`Zlib`/`CC0-1.0` 허용, Pull은 `--ff-only`, Stash는 tracked만, 키 매핑, 폭 임계값(70/100), 상세 분할 최소 높이(30), `--date-order`, UI 영어, `F`=Fetch all.
