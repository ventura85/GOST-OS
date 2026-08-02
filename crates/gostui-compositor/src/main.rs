//! The GostUI compositor.
//!
//! M1 step 2: `--backend winit` opens a real window through smithay. There is
//! still no wayland socket and no client — that is M2. Without a backend the
//! binary stays what M0 made it: a diagnostic that resolves the configuration
//! and prints the layout `gostui-core` computes, so the model can be inspected
//! without a screen.
//!
//! smithay lives in this crate and nowhere else in the workspace (D-016). This
//! file translates events into `gostui-core` calls and draws the state that
//! comes back; it does not decide anything itself.

mod backend;
mod clock;
#[cfg(feature = "winit")]
mod input;
#[cfg(feature = "winit")]
mod render;
#[cfg(feature = "winit")]
mod stats;
#[cfg(feature = "winit")]
mod wayland;

use gostui_core::{layout, Gaps, Outputs, Rect, Size, Split, SurfaceRole, TabStrip};

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return 0;
    }
    if let Some(i) = args.iter().position(|a| a == "--png") {
        match args.get(i + 1) {
            Some(path) => render_png(path),
            None => {
                eprintln!("error: --png needs a file path");
                return 2;
            }
        }
        return 0;
    }
    if let Some(i) = args.iter().position(|a| a == "--backend") {
        let Some(name) = args.get(i + 1) else {
            eprintln!("error: --backend potrzebuje nazwy (winit albo udev)");
            return 2;
        };
        let Some(chosen) = backend::Backend::parse(name) else {
            eprintln!("error: nieznany backend `{name}` — jest `winit` i `udev`");
            return 2;
        };
        let frames = match args.iter().position(|a| a == "--frames") {
            None => None,
            Some(i) => match args.get(i + 1).and_then(|n| n.parse::<u64>().ok()) {
                Some(n) => Some(n),
                None => {
                    eprintln!("error: --frames potrzebuje liczby klatek");
                    return 2;
                }
            },
        };
        let renderer = match args.iter().position(|a| a == "--renderer") {
            None => backend::RendererKind::default(),
            Some(i) => match args
                .get(i + 1)
                .map(String::as_str)
                .and_then(backend::RendererKind::parse)
            {
                Some(kind) => kind,
                None => {
                    eprintln!("error: --renderer przyjmuje `gles2` albo `pixman`");
                    return 2;
                }
            },
        };
        let idle_test = match args.iter().position(|a| a == "--idle-test") {
            None => None,
            Some(i) => match args.get(i + 1).and_then(|n| n.parse::<f64>().ok()) {
                Some(s) if s > 0.0 => Some(std::time::Duration::from_secs_f64(s)),
                _ => {
                    eprintln!("error: --idle-test potrzebuje liczby sekund większej od zera");
                    return 2;
                }
            },
        };
        if frames.is_some() && idle_test.is_some() {
            // Both end the run, and the one that fires first would silently
            // decide what the other one measured.
            eprintln!("error: --frames i --idle-test wykluczają się");
            return 2;
        }
        return backend::run(chosen, renderer, frames, idle_test);
    }

    let (config, problem) = match gostui_config::default_path() {
        Some(path) => {
            let (cfg, err) = gostui_config::Config::load_or_default(&path);
            println!("config:   {}", path.display());
            (cfg, err)
        }
        None => {
            println!("config:   <no HOME set, using defaults>");
            (gostui_config::Config::default(), None)
        }
    };
    if let Some(err) = problem {
        // A broken config is reported and survived, never fatal.
        eprintln!("warning:  {err} — falling back to defaults");
    }

    let gaps = Gaps {
        outer: config.layout.outer_gap,
        inner: config.layout.inner_gap,
    };
    let split = Split::from_permille(config.layout.split_permille);

    let mut outputs = Outputs::new();
    outputs.add("simulated-monitor", Size::new(1920, 1080));
    outputs.add("simulated-phone", Size::new(720, 1600));

    println!("\noutputs:");
    for output in outputs.iter() {
        let logical = output.logical_size();
        let orientation = if output.is_portrait() {
            "portrait"
        } else {
            "landscape"
        };
        // Both bars are 40 logical units tall; the rest belongs to windows.
        let area = Rect::new(0, 40, logical.w, (logical.h - 80).max(0));
        let limit = layout::tile_limit(area, gaps);

        println!(
            "  {:<18} {:>5}x{:<5} {:<9} scale {}  tiles up to {}",
            output.name, logical.w, logical.h, orientation, output.scale, limit
        );
        for (i, tile) in layout::tile(area, limit, split, gaps).iter().enumerate() {
            println!(
                "      tile {}: {:>4},{:<4}  {:>4}x{:<4}",
                i + 1,
                tile.x(),
                tile.y(),
                tile.w(),
                tile.h()
            );
        }
        // A "Save as" dialog is not a third tile (D-025).
        let first = layout::tile(area, limit, split, gaps);
        if let Some(tile) = first.first() {
            let placement =
                layout::placement(SurfaceRole::Dialog, Size::new(600, 400), *tile, false);
            println!("      dialog placement: {placement:?}");
        }
    }

    let mut tabs = TabStrip::new();
    for name in ["Pliki", "Praca", "Rozrywka"] {
        tabs.add(name);
    }
    let names: Vec<_> = tabs.iter().map(|t| t.name.as_str()).collect();
    println!("\ntabs:     {}", names.join(" · "));
    println!(
        "active:   {}",
        tabs.active().map(|t| t.name.as_str()).unwrap_or("<none>")
    );

    println!("\nOkno kompozytora: --backend winit. Klientów jeszcze nie ma (M2).");
    0
}

/// Load the user's theme, reporting anything that had to be corrected (D-032).
///
/// Never fails: a missing file is a first run and a broken one falls back to the
/// built-in theme, because appearance is not worth refusing to start over.
///
/// `Pointing::Pointer` is fixed here because both callers run on a desktop with
/// a mouse — the PNG preview and the nested window. Deciding it from the input
/// devices actually present needs `wl_pointer`, which arrives with clients in M2.
fn load_theme() -> gostui_core::Theme {
    let Some(path) = gostui_config::theme::default_path() else {
        return gostui_core::Theme::builtin();
    };
    let (theme, report) = gostui_config::theme::load(&path, gostui_core::Pointing::Pointer);
    for line in report.lines() {
        eprintln!("theme: {}: {line}", path.display());
    }
    theme
}

/// Render the shell to PNG files with the software rasteriser.
///
/// No compositor and no GPU involved. Two images are produced from the *same*
/// state — a monitor and a docked phone screen — because the point of D-026 is
/// that one session serves both.
///
/// The clock makes these files stop being reproducible, which is deliberate:
/// this is a preview of the running shell, not the golden image. The tests that
/// compare pixels draw the shell with no clock, for exactly this reason.
fn render_png(path: &str) {
    use gostui_core::shell::zones;
    use gostui_render::{paint, Canvas, ShellView};

    let mut tabs = TabStrip::new();
    for name in ["Pliki", "Praca", "Rozrywka"] {
        tabs.add(name);
    }
    tabs.activate_next();
    let windows = vec!["Terminal".to_string(), "Firefox".to_string()];
    let theme = load_theme();
    // One text renderer for both outputs: the font database is the expensive
    // part, and the glyph cache makes the second drawing of the same clock free.
    let mut text = gostui_render::TextRenderer::new();
    if text.is_fontless() {
        eprintln!("uwaga: brak czcionek w systemie — tekst nie zostanie narysowany");
    }
    let now = gostui_core::clock::format(clock::now_local(), gostui_core::ClockFormat::H24);

    let targets: [(&str, i32, i32, i32); 2] =
        [("monitor", 1920, 1080, 1), ("telefon", 360, 800, 2)];

    for (name, w, h, scale) in targets {
        let area = Rect::new(0, 0, w, h);
        let view = ShellView {
            // Bar heights come from the theme, not from a default: they are one
            // of the sizes the user owns (D-032).
            zones: zones(area, theme.metrics.bar_heights()),
            tabs: &tabs,
            windows: &windows,
            focused_window: Some(0),
            clock: Some(&now),
            // The diagnostic picture has no clients by definition: it draws the
            // shell, and a client's pixels are not ours to invent.
            surfaces: &[],
        };
        let Some(mut canvas) = Canvas::new(w, h, scale) else {
            eprintln!("error: {name} has a degenerate size");
            continue;
        };
        paint(&mut canvas, &view, &theme, &mut text, scale);

        let out = if targets.len() > 1 {
            let stem = path.strip_suffix(".png").unwrap_or(path);
            format!("{stem}-{name}.png")
        } else {
            path.to_string()
        };
        match canvas.write_png(std::path::Path::new(&out)) {
            Ok(()) => println!(
                "{out}  ({}x{} px, skala {scale}, {w}x{h} jednostek logicznych)",
                canvas.width(),
                canvas.height()
            ),
            Err(e) => eprintln!("error: nie udalo sie zapisac {out}: {e}"),
        }
    }
}

fn print_help() {
    println!("gostui — GOST OS shell (M1: okno kompozytora, bez klientow)");
    println!();
    println!("USAGE:  gostui [--backend <nazwa>] [--png <sciezka>] [--help]");
    println!();
    println!("Bez argumentow: wypisuje konfiguracje i layout policzony dla monitora");
    println!("i ekranu telefonu.");
    println!();
    println!("  --backend winit   otwiera okno w biezacej sesji (tryb codzienny).");
    println!("  --backend udev    DRM/KMS na tty — dopiero w M4.");
    println!("  --renderer gles2  rysowanie na GPU (domyslne).");
    println!("  --renderer pixman rysowanie na CPU wlasnym rasteryzerem — rownorzedna");
    println!("                    sciezka, nie awaryjna (D-027).");
    println!("  --frames <n>      zamyka okno po n klatkach (test dymny backendu).");
    println!("  --idle-test <s>   trzyma okno s sekund, nic nie ruszajac, i konczy sie");
    println!("                    bledem, jesli sesja zazadala choc jednej przerysowki.");
    println!("                    Kryterium \"zero renderowania w spoczynku\" jako kod wyjscia.");
    println!("  --png <sciezka>   rysuje trzy strefy i slider kart rasteryzerem");
    println!("                    software'owym; zapisuje dwa pliki (monitor i telefon).");
    println!("                    Bez GPU i bez kompozytora.");
    println!();
    println!("GOSTUI_STATS=1    linia na kazda narysowana klatke (numer, powod, czas");
    println!("                  renderu) plus raport przy zamknieciu. W spoczynku milczy —");
    println!("                  zero klatek to zero linii, bez zadnego dodatkowego budzenia.");
    println!("RUST_LOG=gostui=debug,smithay=debug — logi backendu.");
}
