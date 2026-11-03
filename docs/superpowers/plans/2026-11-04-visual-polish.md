# terminal-git-graph 시각 개선 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 글자 모드를 GUI 클라이언트처럼 다듬고(알약 배지, 헤더, 칼럼 헤더, 상세), 지원 터미널에서는 그래프 레인을 Kitty graphics 이미지로 그리는 선택형 그래픽 모드를 추가한다.

**Architecture:** 기존 ratatui UI 위에 (1) 글자 모드 스타일링 개선과 (2) 새 `gfx` 모듈(프로토콜 인코더, 감지, 래스터라이저, 행 이미지 캐시/프레젠터)을 얹는다. 그래픽 모드는 프레임 직후 훅 하나에서 이미지 배치를 만들고 지운다. 모든 실패는 글자 모드로 돌아간다.

**Tech Stack:** Rust, ratatui 0.29, crossterm 0.28(기존), 새로 추가: `libc`(unix 전용, 터미널 질의용). 래스터라이저는 직접 작성한다.

**Spec:** `docs/superpowers/specs/2026-11-04-visual-polish-design.md` (기반: `docs/superpowers/specs/2026-11-03-terminal-git-graph-design.md`)

**Setup:** CLAUDE.md 규칙에 따라 worktree `../feature-visual-polish`(브랜치 `feature/visual-polish`)에서 작업한다(이미 생성됨).

## Global Constraints

- MIT, 코드 직접 작성. 원본 herdr-git-graph/vscode-git-graph 소스 복사 금지. 원본 스크린샷(`reference-git-graph.png`, `preview.png`)은 저장소에 커밋하지 않고 문서/README에 넣지 않는다.
- 의존성은 MIT/Apache-2.0/BSD/ISC(+ Unicode-3.0, Zlib, CC0-1.0)만. `cargo deny check licenses`가 통과해야 한다. 새 크레이트는 `libc`만(필요 시 성능 문제가 확인될 때만 `flate2`).
- 글자 모드는 항상 모든 기능을 갖추며 모든 터미널에서 동작한다. 터미널 전용 기능은 **감지에 성공했을 때만** 쓴다.
- 이미지 프로토콜은 Kitty graphics(`ESC _ G … ESC \`)만 쓴다. 텍스트를 이미지로 그리지 않는다(폰트 래스터화 금지).
- 감지는 이벤트 루프 시작 전에, 최대 300ms 안에 끝나고, 입력 버퍼에 남는 바이트가 없어야 한다. 힌트(환경 변수)만으로 그래픽을 켜지 않는다.
- 설정은 기존 `~/.config/terminal-git-graph/config.toml`에 `renderer = "auto" | "text" | "graphics"`(기본 `auto`)와 `nerd_font`(기본 `false`)를 둔다. 잘못된 값은 경고 + 기본값. CLI 플래그 없음.
- 새 글리프는 `unicode-width`의 `width()`와 `width_cjk()`가 모두 1이어야 한다. 기존 `●`, `◯`, `✓`는 유지한다.
- 메뉴/대화상자/"Terminal too small" 중에는 이미지 배치를 모두 지우고 그래프 열을 글자 모드로 그린다. 종료·패닉 시 이미지를 모두 지운다. 이미지 쓰기 실패는 앱 종료가 아니라 글자 모드 전환이다.
- 행 이미지는 투명 배경, 원시 RGBA(`f=32`), 캐시 키에 선택 상태를 넣지 않는다.
- UI 문자열은 영어. macOS 우선(Linux 가능), Windows 비목표.

## Review Focus

1. **응답하지 않는 터미널**(질의에 답이 없음): 300ms 안에 글자 모드로 시작하고 입력 바이트가 남지 않는다. (Task 6)
2. **그래픽 모드 중 메뉴/대화상자/Too small**: 이미지가 남거나 비쳐 보이지 않는다. (Task 9)
3. **리사이즈와 셀 크기 변화**: 캐시가 무효화되고, 크기를 모르게 되면 글자 모드. (Task 8, 9)
4. **이미지 쓰기 실패**(깨진 파이프 등): 앱이 죽지 않고 글자 모드로 간다. (Task 9)
5. **종료 경로(q, 패닉, 오류 종료)**: 터미널에 이미지가 남지 않는다. (Task 9)
6. **잘못된 설정 값**(`renderer = "fancy"`): 경고 후 `auto`. (Task 1)

---

### Task 1: 설정 키 `renderer`, `nerd_font`

**Files:**
- Modify: `src/config.rs`
- Test: `tests/config.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum RendererSetting { Auto, Text, Graphics }
  // Config gains: pub renderer: RendererSetting (default Auto), pub nerd_font: bool (default false)
  ```
  `Config::load`는 기존과 같은 시그니처. `renderer` 문자열이 `auto|text|graphics`가 아니면 해당 키만 `Auto`로 두고 경고 문자열을 반환한다(파일 전체를 버리지 않는다).

- [ ] **Step 1: 실패하는 테스트** — `renderer_defaults_to_auto`; `renderer_values_parse`(세 값 모두); **`invalid_renderer_warns_and_uses_auto`**(`renderer = "fancy"` → `Auto` + 경고에 `renderer` 포함, 같은 파일의 `scan_depth = 4`는 유지); `nerd_font_defaults_false_and_parses_true`.
- [ ] **Step 2: 실패 확인** — `cargo test --test config` → FAIL.
- [ ] **Step 3: 구현** — `RawConfig`에 문자열 `renderer`와 `nerd_font` 추가, 변환 함수 `parse_renderer(&str) -> Option<RendererSetting>`.
- [ ] **Step 4: 통과 확인** — `cargo test --test config` PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: renderer and nerd_font settings"`

---

### Task 2: 글자 모드 — 글리프 세트와 알약 배지

**Files:**
- Create: `src/ui/glyphs.rs`
- Modify: `src/ui/table.rs`(배지), `src/ui/mod.rs`(모듈 등록), `src/app.rs`(`nerd_font` 보관 및 `set_nerd_font`), `src/main.rs`(설정 전달), `src/graph/render.rs`(노드 기호)
- Test: `tests/ui_badges.rs`

**Interfaces:**
- Consumes: Task 1 `Config.nerd_font`.
- Produces:
  ```rust
  pub struct BadgeGlyphs { pub left_cap: &'static str, pub right_cap: &'static str, pub check: &'static str, pub local: &'static str, pub remote: &'static str, pub tag: &'static str }
  pub fn badge_glyphs(nerd_font: bool, ascii: bool) -> BadgeGlyphs;     // ascii면 모두 ASCII 또는 빈 문자열
  impl App { pub fn set_nerd_font(&mut self, on: bool) }
  ```
  배지 스타일(스펙 §5.1): 체크아웃한 로컬 브랜치 = 강조색 채운 알약 + ✓, 다른 로컬 브랜치 = 연한 배경, 원격/태그 = 어두운 틴트 배경 + 색 글자. 종류별 아이콘을 앞에 붙인다. `nerd_font`면 알약 끝 글리프(둥근 캡)를 쓰고, 아니면 앞뒤에 공백 1칸 패딩의 사각 알약이다. 배지 폭 계산, 클릭 영역(`hit`), `…` 자르기는 기존 `badge_spans`와 같은 규칙을 유지한다.

- [ ] **Step 1: 실패하는 테스트** (`tests/ui_badges.rs`, TestBackend 버퍼): `checked_out_branch_badge_is_filled_with_check`; `remote_and_tag_badges_have_distinct_icons`; **`all_new_glyphs_are_single_width`**(`badge_glyphs(false,false)`와 `(true,false)`의 모든 아이콘/캡이 `width()==1 && width_cjk()==1`; 캡/아이콘이 빈 문자열이면 건너뜀); `ascii_glyphs_are_ascii_only`; `nerd_font_adds_round_caps`; `badge_click_areas_match_drawn_width`(기존 `hit` 테스트와 같은 방식으로 라벨 위치 클릭); `long_badge_is_cut_with_ellipsis_within_budget`.
- [ ] **Step 2: 실패 확인** — `cargo test --test ui_badges` → FAIL.
- [ ] **Step 3: 구현** — `badge_spans`를 `BadgeGlyphs`와 새 스타일로 확장한다. 노드 기호 변경은 `render.rs`의 글리프 테이블에서만 하되 폭 1칸이며 `tests/graph_render.rs`를 갱신한다(기존 ●/◯는 유지하고 변경하지 않아도 된다 — 이번 작업에서는 바꾸지 않는다).
- [ ] **Step 4: 통과 확인** — `cargo test` 전체 PASS(기존 `ui_table`, `graph_render`, `input`의 라벨 클릭 테스트 포함).
- [ ] **Step 5: 커밋** — `git commit -m "feat: pill badges with kind icons"`

---

### Task 3: 글자 모드 — 칼럼 헤더 행과 분절형 헤더 컨트롤

**Files:**
- Modify: `src/ui/table.rs`(칼럼 헤더 행, 행 영역 1줄 감소), `src/ui/header.rs`(분절형 컨트롤, 체크박스), `src/ui/mod.rs`(레이아웃), `src/ui/layout.rs`
- Test: `tests/ui_header.rs`

**Interfaces:**
- Consumes: 기존 `Columns`, `columns_for`, `ToolbarButton`, `Hit`.
- Produces: 표 영역 맨 위에 흐린 칼럼 헤더 한 줄(`Graph`, `Description`, `Date`, `Author`, `Commit` 중 **표시되는 열만**, 각 열 위치에 정렬). 헤더는 `All branches | Current` 분절형(선택된 쪽 반전), `☑ Remotes`/`☐ Remotes`, 우측에 커밋 수와 Fetch 버튼. 폭이 좁으면 기존처럼 항목을 줄인다. 클릭 영역(`button_at`)은 새 위치와 일치해야 한다. 칼럼 헤더 행은 행 인덱스에 포함되지 않으며 클릭해도 아무 일도 없다.

- [ ] **Step 1: 실패하는 테스트**: `column_header_shows_only_visible_columns`(폭 50은 Graph/Description/Date, 폭 120은 Author/Commit 추가); `column_header_aligns_with_row_columns`(헤더 `Date` 시작 열 == 행의 날짜 시작 열); `segmented_control_highlights_active_scope`; `remotes_checkbox_reflects_state`; `toolbar_click_targets_follow_new_layout`(Scope, Remotes, Fetch 각각 `button_at` 위치 클릭); `row_click_ignores_column_header`; `narrow_header_drops_items_without_overflow`(폭 30).
- [ ] **Step 2: 실패 확인** — `cargo test --test ui_header` → FAIL.
- [ ] **Step 3: 구현** — `table::draw`가 첫 줄을 헤더로 쓰고 행 영역을 `rows_layout`에서 한 줄 줄인다. 선택/스크롤/페이징 계산(`visible_rows`, `hit`)이 새 오프셋을 쓰는지 확인한다.
- [ ] **Step 4: 통과 확인** — `cargo test` 전체 PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: column header row and segmented header controls"`

---

### Task 4: 글자 모드 — 상세 패널 정돈

**Files:**
- Modify: `src/ui/detail.rs`
- Test: `tests/ui_detail.rs`(추가)

**Interfaces:**
- Consumes: `FileChange`, `CommitDetail`. 상태 글자 색, 해시 강조, 라벨 흐림은 스타일만 바꾸고 텍스트 배치는 유지한다(기존 테스트가 텍스트로 검증).
- Produces: 라벨 열(`Commit`, `Parents`, `Author`, `Date`)은 흐리게(DIM), 전체 해시는 강조색, 제목은 굵게, 파일 상태 글자 `A`는 초록/`M`은 노랑/`D`는 빨강/그 외 기본색, `+N`은 초록, `-N`은 빨강. 파일 목록 앞에 `N file(s)` 요약 행의 `+총합 -총합`도 같은 색.

- [ ] **Step 1: 실패하는 테스트**: `detail_labels_are_dimmed`(`Author` 셀의 스타일에 DIM); `hash_uses_accent_color`; `file_status_letters_are_colored`(A/M/D 각각 fg 색 상이); `added_and_deleted_counts_are_green_and_red`; `subject_is_bold`. 버퍼 셀 스타일로 검증한다.
- [ ] **Step 2: 실패 확인** — `cargo test --test ui_detail` → FAIL.
- [ ] **Step 3: 구현** — 스타일만 변경.
- [ ] **Step 4: 통과 확인** — `cargo test` 전체 PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: styled commit detail panel"`

---

### Task 5: Kitty graphics 인코더

**Files:**
- Create: `src/gfx/mod.rs`, `src/gfx/kitty.rs`
- Modify: `src/lib.rs`(`pub mod gfx;`)
- Test: `tests/gfx_kitty.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)] pub struct ImageId(pub u32);
  pub fn transmit_rgba(id: ImageId, width: u32, height: u32, rgba: &[u8]) -> Vec<u8>;      // a=t,f=32,q=2, base64를 4096자 이하 청크(m=1…m=0)
  pub fn place(id: ImageId, placement: u32, cols: u16, rows: u16) -> Vec<u8>;              // a=p,i,p,c,r,q=2,C=1(커서 이동 없음)
  pub fn delete_all_placements() -> Vec<u8>;                                                // a=d,d=a,q=2
  pub fn delete_image(id: ImageId) -> Vec<u8>;                                              // a=d,d=I,i=id,q=2
  pub fn move_to(col: u16, row: u16) -> Vec<u8>;                                            // 0-based 입력 → CSI row+1 ; col+1 H
  ```
  모든 출력은 `ESC _ G <키=값,…> ; <payload> ESC \` 형식이고 바이트 배열로 반환된다.

- [ ] **Step 1: 실패하는 테스트** (정확한 바이트 비교): `transmit_small_image_single_chunk`(2×1 RGBA → `"\x1b_Ga=t,f=32,s=2,v=1,i=7,q=2,m=0;<base64>\x1b\\"` 형태; 키 순서는 구현이 정하되 테스트가 문자열을 고정한다); `transmit_large_image_is_chunked_at_4096`(청크마다 payload ≤ 4096, 마지막 외 `m=1`, 이어붙인 payload를 디코딩하면 원본 일치); `place_has_no_cursor_movement_flag`(`C=1` 포함); `delete_all_and_delete_image_sequences`; `move_to_is_one_based`(`move_to(0,0) == b"\x1b[1;1H"`); `rgba_length_mismatch_panics_or_errors`(길이가 `w*h*4`가 아니면 `debug_assert`/Result — 구현이 Result를 반환하도록 시그니처를 `Result<Vec<u8>, GfxError>`로 바꿔도 된다).
- [ ] **Step 2: 실패 확인** — `cargo test --test gfx_kitty` → FAIL.
- [ ] **Step 3: 구현** — 기존 `base64` 크레이트 사용.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: kitty graphics protocol encoder"`

---

### Task 6: 그래픽 지원 감지

**Files:**
- Create: `src/gfx/detect.rs`
- Modify: `Cargo.toml`(`libc`, unix 전용 `[target.'cfg(unix)'.dependencies]`), `src/gfx/mod.rs`
- Test: `tests/gfx_detect.rs`

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, PartialEq, Eq, Debug)] pub struct CellPx { pub w: u16, pub h: u16 }
  #[derive(Debug, PartialEq, Eq)] pub struct ProbeReply { pub kitty_ok: bool, pub cell: Option<CellPx>, pub done: bool }
  pub fn parse_probe_reply(buf: &[u8]) -> ProbeReply;            // Kitty 응답 `ESC _ G i=31;OK ESC \`, 셀 크기 `ESC [ 6 ; h ; w t`, 종료 표지 DA1 `ESC [ ? … c`
  pub fn probe_request() -> Vec<u8>;                              // Kitty 질의(i=31) + `ESC[16t` + `ESC[c`
  pub fn cell_px_from_window(pixels: (u16, u16), cells: (u16, u16)) -> Option<CellPx>;   // 0이면 None
  pub fn env_blocks_graphics(tmux: bool, term: Option<&str>) -> bool;   // tmux 안이거나 TERM=dumb|linux면 true (질의 자체를 건너뜀)
  #[cfg(unix)] pub fn probe_terminal(timeout: std::time::Duration) -> Option<CellPx>;   // 성공(kitty_ok && 셀 크기 알 수 있음)일 때만 Some
  ```
  `probe_terminal`은 `/dev/tty`를 열어 termios raw(echo 끔)로 요청을 쓰고 `poll`로 응답을 읽는다. DA1 응답(`done`)이 오거나 `timeout`(300ms)이 지나면 끝낸다. 읽은 바이트는 모두 이 함수가 소비하고, 종료 전에 termios를 복원한다. 셀 크기는 `CSI 16 t` 응답을 우선하고, 없으면 `crossterm::terminal::window_size()`로 `cell_px_from_window`를 쓴다.

- [ ] **Step 1: 실패하는 테스트** (바이트 파서만; 실제 tty 테스트는 없음): `parses_kitty_ok_then_cell_then_da1`(합친 바이트열); `kitty_error_reply_is_not_ok`(`ESC _ G i=31;ENOTSUPPORTED:… ESC \`); **`only_da1_reply_means_unsupported`**(Kitty 응답 없이 DA1만 → `kitty_ok=false, done=true`); `partial_reply_is_not_done`; `cell_size_reply_parsed`(`ESC[6;20;10t` → h=20,w=10); `window_pixels_zero_is_none`; `cell_px_division`(`(1600,800)/(160,40)` → w=10,h=20); `env_blocks_tmux_and_dumb`; `probe_request_ends_with_da1`.
- [ ] **Step 2: 실패 확인** — `cargo test --test gfx_detect` → FAIL.
- [ ] **Step 3: 구현** — `libc::{open,read,poll,tcgetattr,tcsetattr,cfmakeraw}`. 모든 `unsafe`는 `probe_terminal`의 작은 블록에 가둔다. 타임아웃은 남은 시간으로 `poll`한다.
- [ ] **Step 4: 통과 확인** — `cargo test --test gfx_detect` PASS, `cargo deny check licenses` 통과.
- [ ] **Step 5: 커밋** — `git commit -m "feat: kitty graphics capability detection"`

---

### Task 7: 레인 래스터라이저

**Files:**
- Create: `src/gfx/raster.rs`
- Test: `tests/gfx_raster.rs`

**Interfaces:**
- Consumes: `GraphRow`, `LaneSeg`, `PALETTE_LEN`, `RenderOpts { is_head, ascii }`, `CellPx`.
- Produces:
  ```rust
  #[derive(Clone, Copy, PartialEq, Eq, Debug)] pub struct Rgb(pub u8, pub u8, pub u8);
  pub struct RasterStyle { pub cell: CellPx, pub palette: [Rgb; PALETTE_LEN], pub line_px: f32, pub node_radius_px: f32 }
  pub fn row_size(row: &GraphRow, style: &RasterStyle) -> (u32, u32);                 // (row.width * 2 * cell.w, cell.h)
  pub fn rasterize_row(row: &GraphRow, opts: &RenderOpts, style: &RasterStyle) -> Vec<u8>;   // RGBA, 투명 배경, 병합 행은 알파를 낮춰 흐리게
  pub fn rasterize_uncommitted(head_lane: usize, width: usize, style: &RasterStyle) -> Vec<u8>;   // 점선 세로선 + 작은 빈 원
  pub fn row_key(row: &GraphRow, opts: &RenderOpts, style: &RasterStyle) -> u64;     // 내용이 같으면 같은 키(선택 상태는 입력이 아님)
  ```
  기하: 레인 `i`의 중심 x는 `i*2*cell.w + cell.w/2`. 노드는 `row.lane`의 중심 y=`cell.h/2`에 원(HEAD 행은 속이 빈 링). `pass`는 위에서 아래까지 수직선, `continues`면 노드에서 아래로, `row.lane`과 같은 레인이 위에서 이어지는 경우를 위해 노드 위쪽 선도 그린다(루트가 아니고 위에서 내려오는 경우 — 기존 `render_row`와 같은 규칙을 따른다). `joins`와 `forks`는 노드에서 해당 레인 중심으로 이어지는 부드러운 곡선(3차 베지어, 위/아래 경계 y에서 수직으로 끝남). 선은 안티앨리어싱한다. 색은 `palette[color]`.

- [ ] **Step 1: 실패하는 테스트** (픽셀 단위, 임의의 `RasterStyle{cell:10×20}`): `output_length_matches_row_size`(`len == w*h*4`); `background_is_transparent`(구석 픽셀 알파 0); `node_center_is_opaque_lane_color`; `head_row_node_is_a_ring`(중심은 투명, 반지름 근처는 불투명); `pass_lane_draws_continuous_vertical_line`(그 레인 중심 x의 모든 y에서 알파 > 0); `join_curve_connects_neighbor_lane_at_top`(위쪽 경계 y=0의 합류 레인 중심 x에서 알파 > 0); `fork_curve_reaches_bottom_in_target_lane`(y=h-1에서 대상 레인 중심 x의 알파 > 0); `merge_row_alpha_is_reduced`(같은 행의 `is_merge` 변형이 노드 중심 알파가 더 낮음); `rasterization_is_deterministic_and_key_stable`(두 번 호출한 결과 동일, `row_key` 동일, 다른 색이면 키 다름); **`seamless_tiling_between_rows`**(`pass` 레인만 있는 두 행을 위아래로 이었을 때 경계 y의 알파가 같다); `uncommitted_row_is_dashed`(수직 픽셀 열에 투명 간격이 존재).
- [ ] **Step 2: 실패 확인** — `cargo test --test gfx_raster` → FAIL.
- [ ] **Step 3: 구현** — 거리장(distance field) 기반 안티앨리어싱: 픽셀 중심에서 선분/곡선(폴리라인 근사)까지의 거리로 알파를 계산한다. 알파 합성은 "over" 연산. 구체 수식은 빌더 재량.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: anti-aliased lane rasterizer"`

---

### Task 8: 행 이미지 캐시와 프레젠터

**Files:**
- Create: `src/gfx/present.rs`
- Test: `tests/gfx_present.rs`

**Interfaces:**
- Consumes: Task 5 인코더, Task 7 `rasterize_row`/`row_key`/`RasterStyle`.
- Produces:
  ```rust
  pub struct RowPlacement { pub col: u16, pub row: u16, pub cols: u16, pub key: u64, pub rgba_w: u32, pub rgba_h: u32 }   // 화면 셀 좌표(0-based)
  pub struct Presenter { /* 캐시: key → ImageId, 마지막으로 배치한 목록, 다음 ID, 현재 CellPx */ }
  impl Presenter {
      pub fn new(max_cached: usize) -> Presenter;                       // 기본 512
      pub fn present(&mut self, out: &mut impl std::io::Write, cell: CellPx, plan: &[RowPlacement], render: &mut dyn FnMut(u64) -> Vec<u8>) -> std::io::Result<()>;
      // 이미 전송된 key는 재전송하지 않는다. 이전 프레임 배치와 같으면 아무것도 쓰지 않는다. 다르면 delete_all_placements 후 보이는 행만 move_to + place.
      pub fn clear(&mut self, out: &mut impl std::io::Write) -> std::io::Result<()>;        // 모든 배치 삭제(이미지 데이터는 캐시에 유지)
      pub fn reset(&mut self, out: &mut impl std::io::Write) -> std::io::Result<()>;        // 모든 배치와 이미지 삭제, 캐시 비움(종료/셀 크기 변경)
  }
  ```
  `render(key)`는 캐시에 없는 키에만 호출되며 RGBA를 반환한다. 캐시가 `max_cached`를 넘으면 가장 오래 쓰지 않은 이미지를 `delete_image`로 지운다. `cell`이 이전과 다르면 `reset` 후 다시 시작한다.

- [ ] **Step 1: 실패하는 테스트** (`Vec<u8>` writer, 출력 바이트에서 `a=t`/`a=p`/`a=d` 개수 센다): `first_present_transmits_then_places_each_visible_row`; `same_plan_twice_writes_nothing_second_time`; `scroll_by_one_row_transmits_only_the_new_row`(기존 키는 `a=t` 없음); `identical_rows_share_one_image`(같은 key 두 행 → `a=t` 1회, `a=p` 2회); `changed_plan_deletes_all_placements_first`; `clear_deletes_placements_but_keeps_cache`(이후 present는 `a=t` 없이 `a=p`만); `cell_size_change_resets_everything`(`a=d,d=I` 또는 전체 삭제 후 재전송); `lru_evicts_oldest_over_limit`(max 2, 3개 키 → `delete_image` 1회, 그 키는 다시 쓰면 재전송); `write_error_is_returned`(항상 실패하는 writer → `Err`).
- [ ] **Step 2: 실패 확인** — `cargo test --test gfx_present` → FAIL.
- [ ] **Step 3: 구현** — 배치마다 `p=` 번호는 행 순번을 쓴다. 커서 위치 저장/복원(`ESC 7`/`ESC 8`)으로 ratatui의 커서 상태를 건드리지 않는다.
- [ ] **Step 4: 통과 확인** — PASS.
- [ ] **Step 5: 커밋** — `git commit -m "feat: row image cache and presenter"`

---

### Task 9: 통합 — 감지, 배치, 정리, 폴백

**Files:**
- Modify: `src/terminal.rs`(`Screen` 트레이트에 훅, 실행 루프, `restore`/패닉 훅에서 이미지 삭제), `src/app.rs`(그래프 열 이미지 배치 계산, 그래픽 활성 플래그), `src/ui/table.rs`(그래픽 활성 시 그래프 열을 공백으로), `src/manygit/view.rs`(Screen 구현에서 현재 화면이 그래프일 때만 배치 제공), `src/main.rs`(감지와 설정 결합)
- Test: `tests/gfx_integration.rs`

**Interfaces:**
- Consumes: Task 1 `RendererSetting`, Task 6 `probe_terminal`/`env_blocks_graphics`, Task 7, Task 8, 기존 `Screen` 트레이트(`draw`, …).
- Produces:
  ```rust
  // terminal.rs — Screen 트레이트에 기본 구현이 있는 메서드 추가
  pub struct GfxFrame { pub placements: Vec<RowPlacement>, pub style: RasterStyle, pub render: Box<dyn FnMut(u64) -> Vec<u8>> }
  pub trait Screen { /* 기존 */ fn gfx_frame(&mut self, area: ratatui::layout::Rect) -> Option<GfxFrame> { None } fn set_gfx_active(&mut self, on: bool) {} }
  // App: pub fn set_gfx_active(&mut self, on: bool); pub fn graph_placements(&self, area: Rect, cell: CellPx) -> Vec<RowPlacement>;
  pub enum GfxMode { Text, Graphics(CellPx) }
  pub fn decide_mode(setting: RendererSetting, probed: Option<CellPx>, env_blocked: bool) -> (GfxMode, Option<String>);   // 두 번째 값: 경고(강제인데 불가능할 때)
  ```
  규칙: `Text` 설정이면 질의하지 않는다. `Auto`/`Graphics`면 `env_blocks_graphics`가 거짓일 때 `probe_terminal(300ms)`를 이벤트 루프 시작 전에 실행한다. `Graphics`인데 불가하면 `(Text, Some("…falling back to text rendering."))`로 시작 후 한 번 안내 대화상자. 이벤트 루프는 `term.draw` 직후 훅 하나에서: 메뉴/대화상자/Too small/그래프가 아닌 화면이면 `presenter.clear`, 아니면 `gfx_frame`으로 `present`. 이 경우 그래프 열 셀은 공백(그래픽 활성 시 `set_gfx_active(true)`), 오버레이 중에는 `set_gfx_active(false)`로 글자 모드 그래프를 그린다(다음 프레임에서 복귀). `present`가 `Err`이면 `reset`을 시도하고 `set_gfx_active(false)`, 모드를 `Text`로 고정하고 오류 메시지를 `refresh_error` 방식의 비치명 경로로 표시한다. 크기 변경 이벤트에서 `window_size()`로 셀 크기를 다시 계산하고(0이면 유지), 바뀌면 `reset`. 종료(`q`), 오류 종료, 패닉 훅의 `restore`는 `delete_all_placements` + 모든 이미지 삭제를 먼저 쓴다.

- [ ] **Step 1: 실패하는 테스트** (`tests/gfx_integration.rs`, 가짜 writer와 `TestBackend`): `decide_mode_table`(Text 설정 → Text, Auto+probed Some → Graphics, Auto+None → Text(경고 없음), Graphics+None → Text+경고, env_blocked → Text); **`unanswered_probe_falls_back_within_budget`**는 `probe`가 아닌 `decide_mode`가 `probed=None`일 때 질의 호출 없이 동작함을 검증하고, 실제 타임아웃은 Task 6 파서 테스트가 책임진다; `graph_cells_are_blank_when_graphics_active`; `placements_cover_exactly_visible_rows`(스크롤 위치에 따라 `row` 좌표가 행 영역과 일치, 칼럼 헤더/Uncommitted 행 포함); `menu_open_clears_placements_and_draws_text_graph`; `dialog_and_too_small_clear_placements`; **`write_error_switches_to_text_mode_without_quitting`**(항상 실패하는 writer); `resize_with_new_cell_size_resets_presenter`; **`exit_paths_emit_delete_sequences`**(정상 종료, 오류 종료 함수가 삭제 바이트를 쓴다; 패닉 훅은 함수를 직접 호출해 검증); `manygit_list_screen_has_no_placements`.
- [ ] **Step 2: 실패 확인** — `cargo test --test gfx_integration` → FAIL.
- [ ] **Step 3: 구현** — 프레임 직후 훅은 한 함수(`present_frame`)로 모아 모든 분기를 거기서 처리한다. 이미지 쓰기는 `term.backend_mut()`의 writer(stdout)로 한다.
- [ ] **Step 4: 통과 확인** — `cargo test` 전체 PASS, `cargo clippy -- -D warnings`.
- [ ] **Step 5: 커밋** — `git commit -m "feat: graphics mode integration with text fallback"`

---

### Task 10: 성능 검증, 문서, 실제 터미널 점검표

**Files:**
- Modify: `tests/perf.rs`, `README.md`, `docs/keys.md`(변경 없으면 생략)
- Create: `docs/visual-modes.md`

**Interfaces:** 없음(검증과 문서).

- [ ] **Step 1: 성능 테스트(ignored)** — `raster_40_rows_under_5ms`(40개 서로 다른 행 래스터, `--release`); `present_scroll_step_under_2ms`(캐시된 행 40개 배치 갱신, `Vec<u8>` writer); `graphics_first_frame_adds_under_100ms_vs_text`(가짜 writer로 40행 첫 전송 시간).
- [ ] **Step 2: 실행** — `cargo test --release -- --ignored`. 기준을 못 넘으면 `flate2` 압축(`o=z`)을 추가해 전송량을 줄이고 테스트를 다시 통과시킨다(그 경우 `deny.toml` 확인).
- [ ] **Step 3: 문서** — `docs/visual-modes.md`: 글자/그래픽 모드 설명, `renderer`/`nerd_font` 설정, 감지 방식과 한계(tmux/zellij는 글자 모드), 자체 스크린샷은 사용자가 캡처해 `docs/`에 추가한다는 안내(이 작업에서는 넣지 않음). `README.md` 기능 목록과 설정 표 갱신. 원본 스크린샷은 언급하지 않는다.
- [ ] **Step 4: 검증** — `cargo test`, `cargo clippy -- -D warnings`, `cargo deny check licenses`, `cargo fmt --check` 모두 통과.
- [ ] **Step 5: 점검표 기록** — `docs/visual-modes.md` 끝에 사용자가 ghostty와 herdr에서 직접 확인할 체크리스트(그래픽이 켜지는지, 스크롤, 메뉴 겹침, 리사이즈, 종료 후 잔상, `renderer = "text"`)를 넣는다.
- [ ] **Step 6: 커밋** — `git commit -m "docs: visual modes and perf checks"`

---

## Self-Review 메모

- **스펙 커버리지**: §5.1 배지(T2), 그래프/노드(T2에서 유지 결정), 칼럼 헤더·헤더 컨트롤(T3), 상세(T4); §5.2 감지(T6), 인코더(T5), 래스터(T7), 캐시/전송(T8), 겹침·리사이즈·종료·오류(T9); 설정(T1); 성공 기준 성능·문서(T10). 비목표(텍스트 이미지화, 배지 이미지, tmux)는 제외.
- **판단 사항(계획 단계의 선택)**: 노드 기호는 이번 작업에서 바꾸지 않는다(스펙 §5.1의 "더 부드러운 기호"는 빌더 재량이므로 보류하고 그래픽 모드의 이미지 노드로 대체). 프레젠터 캐시 기본 512개. 감지 시 환경 차단 조건은 tmux와 `TERM=dumb|linux`.
