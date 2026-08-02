# Zależności systemowe

Lista utrzymywana na bieżąco — odtwarzanie jej przy pakowaniu jest niepotrzebnie żmudne.
Wersją wykonywalną tej listy jest `Dockerfile`; przy zmianie trzeba ruszyć oba pliki.

## Do budowania

| Pakiet (Debian/Ubuntu) | Po co | Od którego etapu |
|---|---|---|
| `build-essential`, `pkg-config` | kompilacja i wykrywanie bibliotek | M0 |
| `libwayland-dev` | protokół Wayland | M1 |
| `libxkbcommon-dev` | mapy klawiatury | M1 (smithay linkuje `xkbcommon` niezależnie od cech) |
| `libegl1-mesa-dev` | renderer GLES2 | M1 |
| czcionki systemowe (`fonts-dejavu-core` albo dowolne) | bez nich powłoka działa, ale nie rysuje tekstu | M1 krok 5 |

**Tekst nie dokłada ani jednej zależności systemowej — sprawdzone, nie założone.** Odruch mówi
„skoro fontconfig, to `libfontconfig-1-dev`", ale `cosmic-text` czyta konfigurację fontconfiga
przez czysto rustowy `fontconfig-parser`, a `ldd` na binarce nie pokazuje ani `libfontconfig`,
ani `libfreetype`. Potrzebne są **same pliki czcionek**, nie biblioteka. To dobra wiadomość
dla obrazu na telefon i dla RPi: o jeden pakiet `-dev` mniej w cross-kompilacji.
| `libgbm-dev`, `libdrm-dev` | bufory i modesetting (backend `udev`) | M4 |
| `libinput-dev`, `libudev-dev` | wejście i wykrywanie urządzeń | M4 |
| `libseat-dev` | dostęp do seatu bez roota | M4 |
| `libsystemd-dev` | logind, sesja | M4 |
| `libdbus-1-dev` | D-Bus (`zbus` używa go pośrednio) | M7 |

Stan na tej stacji (2026-07-30): **wszystkie zainstalowane.**

## Do uruchamiania i diagnostyki

| Pakiet | Po co |
|---|---|
| `seatd` | demon seatu; `sudo systemctl enable --now seatd`, użytkownik w grupie `_seatd` |
| `weston` | klienty testowe `weston-simple-shm` / `weston-simple-egl` + implementacja referencyjna |
| `wayland-utils` | `wayland-info` — pierwsze narzędzie przy „aplikacja się nie uruchamia" |
| `foot` | minimalny terminal wayland-native, podstawowy klient testowy |
| `mousepad` (lub inny klient GTK) | sprawdzenie toolkitu, który **nie** implementuje `xdg-decoration` |
| `xautomation` (`xte`) | wysyłanie zdarzeń wskaźnika do zagnieżdżonego okna przy diagnostyce |

Stan na tej stacji: **wszystkie zainstalowane.**

**Klienci do domknięcia M2 (zainstalowane 2026-08-02):**

| Pakiet | Po co |
|---|---|
| `gtk-4-examples` | `gtk4-demo` — kryterium „GTK4 działa"; sprawdza też, czy klient bez `linux-dmabuf` spada na `wl_shm` |
| `qt6-base-examples` | druga rodzina toolkitów; `widgets/dialogs/standarddialogs` rysuje okno bez własnego nagłówka, więc weryfikuje ramkę fokusu z D-043 |
| `wl-clipboard` | `wl-copy` / `wl-paste` — dzięki nim test schowka jest skryptem (`scripts/test-schowek.sh`), a nie zaznaczaniem myszą |
| `xdg-desktop-portal` + `xdg-desktop-portal-gtk` | GTK3 kieruje przez portal **natywne** okno wyboru pliku |

```bash
sudo apt install gtk-4-examples qt6-base-examples wl-clipboard \
                 xdg-desktop-portal xdg-desktop-portal-gtk
```

**Do samego kryterium „okno wyboru pliku pływa" portal okazał się niepotrzebny — i to jest
informacja, nie ciekawostka.** Portal otwiera okno w **osobnym procesie**, jako toplevel bez
rodzica, więc sprawdza coś innego niż pułapka 2 z D-025. Dialog z rodzicem daje
`Gtk.FileChooserDialog` (nie `FileChooserNative`), który rysuje własne okno z `set_parent` —
i to on wykrył usterkę opisaną w `docs/01` §4, M2 krok 6. Kilkanaście linii w Pythonie przez
`python3-gi` (już zainstalowane) wystarczyło; `xdotool` do klikania nie był potrzebny i nie jest
zainstalowany.

## Narzędzia deweloperskie (cargo)

| Narzędzie | Po co | Stan |
|---|---|---|
| `cargo-deny` | licencje zgodne z GPL-3.0 + podatności; to samo, co robi CI | **zainstalowane** (0.20.2) |

```bash
cargo install cargo-deny --locked     # ~3 min kompilacji
cargo deny check                      # przed każdym commitem ruszającym zależności
```

## Do mierzenia i wirtualizacji (jeszcze nie zainstalowane)

Potrzebne od M4 i przy egzekwowaniu progów z D-027, nie do startu:

```
sudo apt install qemu-system-x86 qemu-utils ovmf \
                 valgrind heaptrack hyperfine \
                 xdg-desktop-portal xdg-desktop-portal-gtk
```

## Zależności Rust

Świadomie utrzymywane w minimalnej liczbie — drzewo zależności jest pozycją budżetową
na starym sprzęcie (D-027).

| Crate | Zależności | Uwaga |
|---|---|---|
| `gostui-core` | **żadnych** | granica D-016: zero `smithay`, zero `wayland-*` |
| `gostui-desktop-entry` | **żadnych** | format `.desktop` to prosty dialekt INI; własny parser jest tańszy niż crate |
| `gostui-config` | `serde`, `toml` | `toml` bez domyślnych funkcji |
| `gostui-render` | `png` | wyłącznie kodowanie; rasteryzacja własna, bo to kilka wypełnień prostokątów |
| `gostui-compositor` | `smithay`, `tracing`, `tracing-subscriber` | **tutaj i nigdzie indziej** (D-016) |

`smithay` idzie z `default-features = false` i jedną cechą `backend_winit` (D-028). Domyślny
zestaw wciąga DRM, GBM, libinput, libseat, Vulkan i XWayland — czyli biblioteki systemowe
potrzebne dopiero od M4. Kolejne cechy dochodzą etapami: `backend_udev` +
`backend_session_libseat` w M4, `xwayland` w M5.

Licencje sprawdzane automatycznie przez `cargo deny check` (`deny.toml`) — wszystko musi być
zgodne z GPL-3.0. Jeden wpis w `ignore`: **RUSTSEC-2026-0196** (`cgmath` bez utrzymania,
przez smithaya, bez wersji naprawionej). To notka o utrzymaniu, nie podatność — uzasadnienie
w `deny.toml` i w D-028.

## Budowanie w kontenerze

`Dockerfile` daje powtarzalne środowisko **kompilacji** — nie uruchamiania. Kompozytor
potrzebuje węzłów DRM/KMS, seatu i surowych urządzeń wejściowych; uruchamianie go w kontenerze
wymagałoby oddania mu tylu uprawnień hosta, że wynik przestaje cokolwiek weryfikować.

```bash
docker build -t gostui-build .
docker run --rm -v "$PWD:/src" gostui-build cargo test --workspace
docker run --rm -v "$PWD:/src" gostui-build \
  cargo build --release --target aarch64-unknown-linux-gnu \
    -p gostui-core -p gostui-config -p gostui-desktop-entry
```

Obraz jest oparty na Debianie trixie (cel wdrożeniowy na PC), nie na Ubuntu ze stacji roboczej,
i ma toolchain ARM64 do cross-kompilacji na Raspberry Pi (D-002) i telefon (D-026).
Wersja Rusta jest przypięta przez `ARG RUST_VERSION`, żeby obraz nie rozjechał się z CI.

**Zweryfikowane 2026-07-30:** obraz się buduje, w środku `rustc 1.96.0`, `clippy 0.1.96`
i `aarch64-linux-gnu-gcc 14.2.0`; `cargo test --workspace` daje w kontenerze te same 68 testów
co na hoście; cross-kompilacja crate'ów logiki przechodzi. `CARGO_TARGET_DIR=/build` trzyma
artefakty poza podmontowanym drzewem — po przebiegu w kontenerze `git status` jest czysty
i cache hosta w `target/` pozostaje nietknięty.

Dwie pułapki, na które się już nadziano — warto ich nie powtarzać:

- `rustup-init` przyjmuje `--component` jako **flagę powtarzalną**, nie listę. Zapis
  `--component rustfmt clippy` wywala instalację; poprawnie jest `-c rustfmt -c clippy`.
- Obraz `-slim` nie ma **żadnych czcionek**. Powłoka to przeżywa — `TextRenderer` wykrywa pustą
  bazę fontów i rysuje wszystko poza tekstem, zamiast się wywalić — a testy rasteryzacji tekstu
  wypisują wtedy komunikat i się pomijają, zamiast czerwienić się z powodu środowiska. Jeśli
  jednak CI ma naprawdę sprawdzać tekst, potrzebuje `fonts-dejavu-core` w obrazie.
- Debian `-slim` **ma** katalog `/usr/share/applications`, ale niemal pusty. Test parsera
  `.desktop` rozróżnia więc „nie ma czego testować" (pomija się) od „pliki są, żaden się
  nie sparsował" (błąd) — bez tego rozróżnienia zepsuty parser przeszedłby niezauważony
  na każdym hoście bez zainstalowanych aplikacji.
