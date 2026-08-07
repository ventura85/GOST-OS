//! Golden images: the shell drawn to pixels and compared against files in the
//! repository.
//!
//! # Why this exists
//!
//! It is a condition of a decision already taken, not a new idea. **D-028**
//! chose to run the nested Pixman path as "CPU into a texture" rather than
//! build a second nested backend, **on the condition that the CPU path is
//! covered by golden PNG tests** — the risk being that a path nobody looks at
//! until M4 rots quietly. The condition went unmet, and it showed: adding the
//! Start Menu icon changed what the shell looks like and not one test noticed.
//!
//! (The commit that introduced this file cites D-010 for that condition. That is
//! wrong — D-010 is about multiple outputs — and the history is public, so the
//! correction lives here rather than in a rewritten message.)
//!
//! # Why pixels and not a snapshot of the display list
//!
//! A display list is cheaper to store and gives a readable diff, and it would
//! have caught the icon. It would not catch what this is actually guarding
//! against: the list is **shared by both renderer paths**, so a fault in the CPU
//! rasteriser leaves it identical. Only pixels see the rasteriser.
//!
//! # Why there is no text in any of these
//!
//! The shell draws three kinds of text — the clock, the caption of every dead
//! tile, and the name in every card's header — and every scene here is built
//! without any of them: `clock: None`, shortcuts with no name, cards with no
//! name. All three are **data**, not a switch that changes how the shell draws:
//! the caption strip is still reserved inside each tile, the icon square is
//! still sized around it, and the header bar is still drawn and still has the
//! name centred in it, so what these images pin down is the layout that ships.
//! Only the glyphs are missing. **Anything new with text goes on this list** —
//! take it away from the scenes with data, never with a flag. That is
//! deliberate twice over: text differs by one bit between the two paths (D-005),
//! and glyphs would tie these files to the font versions of whatever machine
//! rendered them — which would make CI disagree with a developer's laptop for
//! reasons that have nothing to do with the shell. Text has its own tests, of
//! layout and of the cache.
//!
//! # When one fails
//!
//! The test writes what it actually got next to the expected file, as
//! `<name>.actual.png`, and names the path. Look at the two side by side before
//! deciding anything. If the change was intended:
//!
//! ```text
//! GOSTUI_BLESS=1 cargo test -p gostui-render --test golden
//! ```
//!
//! Blessing rewrites the expectations, so it is the one command here that can
//! turn a regression into a fact. Look first.

use std::path::{Path, PathBuf};

use gostui_core::shell::zones;
use gostui_core::tab::LauncherItem;
use gostui_core::theme::Theme;
use gostui_core::{Rect, TabStrip};
use gostui_render::{paint, Canvas, ShellView, TextRenderer};

/// One picture worth keeping: a name, a screen, and the state to draw.
struct Scene {
    name: &'static str,
    /// Logical size. The physical image is this multiplied by `scale`.
    size: (i32, i32),
    scale: i32,
    /// How many cards the strip holds. A **count**, not names: the middle zone
    /// draws the card's name in its header now, and these images stay free of
    /// glyphs the way they stay free of a clock — by the data the scene is
    /// given, never by a switch that changes what is drawn. The header bar is
    /// reserved and the name is centred in it exactly as in production; only
    /// the glyphs are absent.
    cards: usize,
    /// Index of the active tab, applied by cycling from the first.
    active: usize,
    /// Shortcuts given to every card. Without these the cards draw empty and
    /// the tile grid — the largest piece of new geometry in the middle zone —
    /// would have no pixel covering it at all.
    items: usize,
    /// Whether the strip is in edit mode (D-048), which puts a delete button in
    /// every card's header.
    edit: bool,
    windows: &'static [&'static str],
    focused_window: Option<usize>,
}

/// The scenes, chosen so that each one can fail on its own for its own reason.
fn scenes() -> Vec<Scene> {
    vec![
        // The everyday case, and the only one at scale 1: if the cards or the
        // bars move, this is what says so.
        Scene {
            name: "monitor",
            size: (1920, 1080),
            scale: 1,
            cards: 3,
            active: 1,
            items: 5,
            edit: false,
            windows: &["Terminal", "Firefox"],
            focused_window: Some(0),
        },
        // More cards than the output has room for: the last column comes out
        // clipped, and that clipped column is the specification's sliver of the
        // neighbouring card (D-046). Active is past the visible run, so the
        // strip has to have scrolled — if `first` ever stops being derived from
        // the active card, this picture says so.
        Scene {
            name: "monitor-przewiniety",
            size: (1920, 1080),
            scale: 1,
            cards: 9,
            active: 8,
            items: 3,
            edit: false,
            windows: &[],
            focused_window: None,
        },
        // A phone in portrait at scale 2. Guards two things at once: that the
        // layout is computed in logical units and multiplied at rasterisation
        // (D-011), and that a narrow bar drops the right elements.
        Scene {
            name: "telefon-pion",
            size: (360, 800),
            scale: 2,
            cards: 2,
            active: 0,
            items: 4,
            edit: false,
            windows: &["Terminal"],
            focused_window: Some(0),
        },
        // Narrow enough that the top bar has to sacrifice elements in the
        // documented order — search, then the clock, then status. The Start Menu
        // never goes, and a picture is a blunt way of proving it.
        Scene {
            name: "waski",
            size: (420, 320),
            scale: 1,
            cards: 1,
            active: 0,
            items: 2,
            edit: false,
            windows: &[],
            focused_window: None,
        },
        // Nothing open. The state a fresh session starts in, and the one most
        // likely to be broken by a change that assumes there is always a card.
        Scene {
            name: "pusty",
            size: (1280, 720),
            scale: 1,
            cards: 0,
            active: 0,
            items: 0,
            edit: false,
            windows: &[],
            focused_window: None,
        },
        // Edit mode (D-048): every card's header carries a delete button, and
        // the name gives up that much room. Its own scene rather than a change
        // to an existing one, because the mode has to be told apart from the
        // shell that is merely being used — and because the button is the only
        // destructive control the shell draws, which makes "did it move?" a
        // question worth a picture.
        Scene {
            name: "tryb-edycji",
            size: (1920, 1080),
            scale: 1,
            cards: 3,
            active: 1,
            items: 5,
            edit: true,
            windows: &[],
            focused_window: None,
        },
    ]
}

fn render(scene: &Scene) -> Canvas {
    let mut tabs = TabStrip::new();
    for _ in 0..scene.cards {
        let id = tabs.add("");
        let tab = tabs.get_mut(id).expect("just added");
        for i in 0..scene.items {
            // Nameless on purpose, the same way every scene sets `clock: None`:
            // the caption strip is still reserved and the icon square is still
            // sized around it, so the geometry these images guard is the real
            // one — only the glyphs are absent. See the note on text above.
            tab.items.push(LauncherItem::new(format!("app{i}"), ""));
        }
    }
    for _ in 0..scene.active {
        tabs.activate_next();
    }
    tabs.set_editing(scene.edit);
    let windows: Vec<String> = scene.windows.iter().map(|w| w.to_string()).collect();
    let theme = Theme::default();

    let (w, h) = scene.size;
    let area = Rect::new(0, 0, w, h);
    let view = ShellView {
        zones: zones(area, theme.metrics.bar_heights()),
        tabs: &tabs,
        windows: &windows,
        focused_window: scene.focused_window,
        // No clients: a client's pixels are not ours to invent.
        surfaces: &[],
        // No clock, which is what keeps these files reproducible. See the module
        // comment — this is the point, not an omission.
        clock: None,
    };

    let mut canvas = Canvas::new(w, h, scene.scale).expect("scene has a real size");
    // A text renderer is still required by the signature; with no text in the
    // view it never touches the font database, so a machine with no fonts
    // installed renders these identically.
    paint(
        &mut canvas,
        &view,
        &theme,
        &mut TextRenderer::new(),
        scene.scale,
    );
    canvas
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

/// How many pixels differ, and the first place they do.
fn compare(actual: &[u8], expected: &[u8], width: u32) -> Option<(usize, (u32, u32))> {
    if actual.len() != expected.len() {
        return Some((usize::MAX, (0, 0)));
    }
    let mut differing = 0usize;
    let mut first = None;
    for (i, (a, e)) in actual.chunks(4).zip(expected.chunks(4)).enumerate() {
        if a != e {
            differing += 1;
            if first.is_none() {
                first = Some((i as u32 % width, i as u32 / width));
            }
        }
    }
    first.map(|p| (differing, p))
}

#[test]
fn the_shell_still_looks_the_way_it_is_supposed_to() {
    let dir = golden_dir();
    let blessing = std::env::var_os("GOSTUI_BLESS").is_some();
    if blessing {
        std::fs::create_dir_all(&dir).expect("cannot create tests/golden");
    }

    let mut failures = Vec::new();
    for scene in scenes() {
        let canvas = render(&scene);
        let expected_path = dir.join(format!("{}.png", scene.name));

        if blessing {
            canvas
                .write_png(&expected_path)
                .expect("cannot write the golden image");
            continue;
        }

        let Ok(expected) = std::fs::read(&expected_path) else {
            failures.push(format!(
                "{}: brak wzorca {}. Utwórz go przez `GOSTUI_BLESS=1 cargo test -p gostui-render --test golden` i obejrzyj wynik przed commitem.",
                scene.name,
                expected_path.display()
            ));
            continue;
        };

        // Decoded rather than compared byte for byte as files: PNG encoders are
        // free to choose different filters for the same pixels, and a test that
        // fails when the encoder changes its mind teaches people to bless
        // without looking.
        let expected_pixels = decode_png(&expected)
            .unwrap_or_else(|| panic!("{}: nie udało się odczytać wzorca", scene.name));

        if let Some((count, (x, y))) = compare(canvas.pixels(), &expected_pixels, canvas.width()) {
            let actual_path = dir.join(format!("{}.actual.png", scene.name));
            let _ = canvas.write_png(&actual_path);
            failures.push(format!(
                "{}: {} pikseli się różni, pierwszy w ({x}, {y}). Zobacz {} obok wzorca.",
                scene.name,
                if count == usize::MAX {
                    "inny rozmiar obrazu —".to_string()
                } else {
                    count.to_string()
                },
                actual_path.display()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "wygląd powłoki się zmienił:\n  {}\n\nJeśli to była zmiana zamierzona: \
         GOSTUI_BLESS=1 cargo test -p gostui-render --test golden",
        failures.join("\n  ")
    );

    if blessing {
        // Blessing is not a passing test. Saying so stops a blessed run in CI
        // from ever looking like a green one.
        panic!("wzorce zostały przepisane — uruchom testy jeszcze raz bez GOSTUI_BLESS");
    }
}

/// Reads the pixels back out of a golden file.
///
/// Through the `png` crate, which this crate already depends on to *write* them
/// — and whose manifest comment says why: PNG's zlib stream is not worth
/// reimplementing. A hand-rolled inflate here would be that same mistake, made
/// on the reading side and 250 lines long.
fn decode_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(info.buffer_size());
    Some(buffer)
}
