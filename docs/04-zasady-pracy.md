# GostUI — zasady pracy w tym repozytorium

Dokument jest wiążący dla każdego, kto tu commituje — człowieka i narzędzia. Opisuje, czym jest
projekt, co jest już zrobione, i czego **nie wolno** robić. Zasady, które da się sprawdzić
maszynowo, są sprawdzane maszynowo (§ Higiena repozytorium) — bo zasada, której nikt nie
egzekwuje, jest życzeniem.

## Czym jest ten projekt

Autorski shell / środowisko graficzne dla Linuksa, pisane w Rust na Waylandzie — od zera, nie fork
istniejącego DE. Trzy strefy ekranu: górny pasek (system), środek (slider kart zastępujący pulpit),
dolny pasek (przełącznik okien). Docelowo także własny menedżer plików, menedżer usług i panel
sterowania.

**Stan: M0 i M1 zamknięte, M2 kroki 1–4 zrobione (2026-08-02).** Istnieją i są
przetestowane: `gostui-core` (geometria, wyjścia, **strefy ekranu**, kafelkowanie, **model okien**,
**wejście** — trafienie w strefę i tablica skrótów, D-041, karty, **motyw** — kolory, rozmiary
i czcionki jako dane, D-032),
`gostui-config` (TOML + zapis atomowy), `gostui-desktop-entry` (parser `.desktop` + kody pól
`Exec`), `gostui-render` (rasteryzer software'owy + **tekst przez `cosmic-text`** + zapis PNG),
`gostui-compositor` (backend `winit` na smithayu 0.7 + dwa renderery za wspólnym traitem
+ **gniazdo wayland z `xdg-shell`** + **routing wejścia**). Wszystko poza tym w dokumentacji
to nadal plan, nie stan repozytorium.

**Klienci działają, widać ich okna i można w nich pracować.** `foot` startuje, dostaje kafelek,
rysuje się na obu ścieżkach renderera, kafelkuje się z drugim oknem, przyjmuje pisanie i oddaje
fokus na klik albo na skrót. Zmierzone 2026-08-02: `Super+Q` zamyka okno z fokusem, klik w drugi
kafelek i klik w chip na dolnym pasku przenoszą fokus.
**Trzy rzeczy, o których trzeba wiedzieć, zanim się je zgłosi jako usterkę:** `Super+Tab`
w trybie zagnieżdżonym przechwytuje `xfwm4` (`switch_window_key`) i do nas nie dociera; **kursor
rysuje sesja gospodarza, nie my** (własny wchodzi z M4 — na tty nie ma kto go narysować);
**dotyk ma osobną ścieżkę, ale nieprzetestowaną** — ta stacja nie ma czym jej uruchomić.
Zostaje też `linux-dmabuf` — ścieżka CPU takiego bufora nie odczyta i **świadomie pomija**
takie okno zamiast rysować je źle.

`cargo test --workspace` — 202 testy, bez ekranu i bez GPU. Uruchamiaj po każdej zmianie w core.
`cargo run -p gostui-compositor -- --png ui.png` — rysuje interfejs do dwóch PNG-ów
(monitor i telefon) z tego samego stanu, z zegarem w górnym pasku.
`cargo run -p gostui-compositor -- --backend winit [--renderer gles2|pixman] [--frames n]` —
okno zagnieżdżone. **Tylko `--frames 1` się kończy.** W spoczynku rysuje się dokładnie jedna
klatka (ta początkowa), więc `--frames 5` czeka w nieskończoność na klatki, które nigdy nie
przyjdą — to zero renderowania w spoczynku działające poprawnie, nie awaria. CI tego jeszcze
nie uruchamia; buduje tylko z cechą `winit`.

**Rysowanie jest opisane raz, rasteryzowane dwa razy.** `gostui_render::display_list` daje listę
prymitywów w jednostkach logicznych (`Fill`, `Text`, `Surface`); `TextRenderer::resolve` zamienia tekst
na gotowe obrazy **przed** klatką, a `ShellRenderer` w kompozytorze wykonuje wynik przez
`draw_solid` + tekstury (GLES2) albo przez własny rasteryzer wgrywany jako jedna tekstura (CPU).
**Nie dodawaj rysowania tylko do jednej ścieżki** — obie muszą dawać ten sam obraz.
Nowy element graficzny = nowy wariant w liście wyświetlania, obsłużony po obu stronach.

**Sprawdzone pikselowo, z jednym wyjątkiem, który trzeba znać:** prostokąty są identyczne co do
piksela; tekst różni się na **101 pikselach krawędzi antyaliasingu o 1/255**, bo CPU blenduje
w liczbach całkowitych, a GPU we `float`ach (D-005). Dlatego złote obrazy rysują powłokę **bez
zegara**, a tekst ma własne testy layoutu i cache'u. Nie „naprawiaj" tej różnicy naginaniem
jednej ścieżki do drugiej — jest udokumentowana i ograniczona.

**Zero renderowania w spoczynku jest zmierzone, nie deklarowane.** `GOSTUI_STATS=1` daje linię
na każdą klatkę z **powodem** (`initial`/`resized`/`redraw`/`clock`/`client`/`input`) i raport przy
zamknięciu; `--idle-test <s>` zamienia kryterium w kod wyjścia. Zmierzone 2026-08-01:
**0 klatek bez powodu** na obu ścieżkach, także z otwartym terminalem (2 klatki na 14 s, obie
od klienta); renderowanie zajmuje 0,2–0,3% czasu pracy procesu. Szczegóły i trzy pułapki
tego pomiaru: `docs/01-strategia-dev-test.md` §3.5. **Instrumentacja sama nie może się budzić** —
dlatego wypis jest per klatka, a nie co sekundę, jak mówił pierwotny plan.

**Następny krok: M2 krok 5** — dekoracje i okna nietypowe: `xdg-decoration` (SSD — kafelki nie
rysują własnych ramek), popupy przez `xdg_positioner`, dialogi pływające, pełny ekran.
Kroki M2 z kryteriami: `docs/01-strategia-dev-test.md` §4, sekcja M2.

**Wejście ma dwie granice, tak jak rysowanie.** Co znaczy punkt na ekranie (`hit_test`) i co robi
kombinacja klawiszy (`Keymap`) siedzi w `gostui-core::input` i ma testy bez kompozytora;
`crates/gostui-compositor/src/input.rs` tylko przenosi odpowiedzi do protokołu. Powłoka posiada
**wyłącznie `Super`**, modyfikatory dopasowują się dokładnie, a klawisze przechodzą do core jako
`Keysym(u32)` w numeracji xkb — bez zależności core od `libxkbcommon` (D-041). **Ruch wskaźnika
nie rysuje klatki** — obraz powłoki nie zależy od pozycji kursora, a `request_redraw` na tej
ścieżce to kilkaset klatek na sekundę i koniec z wymaganiem z D-027.

**Okno klienta jest wariantem listy wyświetlania, nie drugim przebiegiem rysowania.**
`Primitive::Surface` niesie **nieprzezroczysty identyfikator** i prostokąt; pikseli szuka
kompozytor — GLES przez teksturę smithaya, CPU przez kopię bufora `wl_shm`. Stąd kolejność Z
jest własnością jednej listy (pulpit → okna → paski) i ma test. `frame` callbacki wysyłaj
**wyłącznie oknom widocznym** — to jedyny bezpiecznik przed aplikacją animującą się w tle.

**Decyzje o oknach zapadają w `WindowModel`, nie w handlerze protokołu.** Handler w
`crates/gostui-compositor/src/wayland/handlers.rs` tłumaczy żądanie na wywołanie modelu i prosi
o przerysowanie — nic więcej. Jeśli piszesz w handlerze `if`, który decyduje, **gdzie** trafia
okno, to jest w złym pliku (D-016). Pojemność kafelków jest **wpychana** do modelu
(`set_capacity` z `layout::tile_limit`), a nie liczona w środku — dzięki temu zwężenie ekranu
spycha okna na dolny pasek i jest to test, nie obserwacja.

**Zegar jest wzorcem dla wszystkiego, co zmienia się samo.** Nie odświeża się co sekundę:
`Wall::until_next_minute` w core mówi, ile spać, a calloop budzi kompozytor dokładnie wtedy, gdy
wyświetlana minuta staje się nieprawdziwa. Kafle żywe z D-033 mają iść tą samą drogą — nigdy
pętla pytająca „czy coś się zmieniło".

**Layout jest logiką, nie rysowaniem.** `gostui-render` nie liczy pozycji: dostaje prostokąty
z `gostui-core` i je wypełnia. Rozmieszczenie elementów górnego paska (`top_bar_layout`)
siedzi w core właśnie dlatego — na wąskim ekranie trzeba zdecydować, co **wypada**, a to
decyzja do przetestowania `cargo test`, nie do obejrzenia.

### Nic nie powstaje na `main` (1/3)
**Każdą pracę zaczynasz od gałęzi.** Nie „gdy zmiana jest duża", nie „gdy nie jestem pewny" —
zawsze. Pierwsza komenda nowego zadania:

```bash
git switch -c m2/krok-5-dekoracje     # <etap>/<krok-lub-temat>
```

Pełne uzasadnienie i reszta rytmu: `§ Higiena repozytorium → Praca na gałęziach`.

### Nic, czego nie uruchomiłem, nie idzie do commita
Zasada z doświadczenia, nie z ostrożności: `Dockerfile` i `deny.toml` trafiły do repozytorium
niesprawdzone i **oba były zepsute**. Przed commitem:

| Co zmieniasz | Co uruchamiasz |
|---|---|
| kod | `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` |
| zależności, `deny.toml`, `Cargo.toml` | `cargo deny check` |
| `Dockerfile` | `docker build -t gostui-build .` |
| formatowanie | `cargo fmt --all` |

CI (`.github/workflows/ci.yml`) robi dokładnie to samo — jeśli przechodzi lokalnie, przejdzie tam.

**A commit i tak nie idzie na `main` (2/3).** Powstaje na gałęzi i wchodzi przez pull request,
z zielonym CI. `main` jest chroniony, więc push wprost i tak zostanie odrzucony — zasada jest
egzekwowana, nie deklarowana.

## Dokumenty — czytaj w tej kolejności

| Plik | Zawartość |
|---|---|
| `gostos.md` | Specyfikacja produktowa od użytkownika. **Źródło prawdy o tym, co budujemy.** Nie zmieniaj bez wyraźnej prośby. |
| `docs/00-przeglad-specyfikacji.md` | Recenzja specyfikacji: luki, blokery, poprawki stacku. Czytaj, zanim zaproponujesz cokolwiek technicznego. |
| `docs/01-strategia-dev-test.md` | Jak budować i testować na stacji użytkownika. Inwentaryzacja sprzętu, cztery poziomy uruchamiania, harmonogram M0–M10 z kryteriami. |
| `docs/02-decyzje.md` | Rejestr ADR. **Sprawdź tu przed każdą decyzją architektoniczną.** |
| `docs/03-cel-telefon.md` | Telefon jako cel docelowy: realia sprzętowe i wymagania dotyku wchodzące już do M1–M3. |

## Język

Użytkownik pisze po polsku i specyfikacja jest po polsku → **odpowiadaj i dokumentuj po polsku**.
Kod, nazwy identyfikatorów, komunikaty commitów i komentarze w kodzie: **po angielsku** (to projekt
open-source-owy z natury i tak jest łatwiej z zależnościami). Teksty widoczne w UI: docelowo i18n,
na razie polski.

## Zasady pracy w tym repozytorium

### Zanim zaproponujesz rozwiązanie techniczne
Sprawdź `docs/02-decyzje.md`. Jeśli decyzja jest oznaczona **OTWARTA**, nie podejmuj jej po cichu
w kodzie — zapytaj albo zaznacz jawnie, że przyjmujesz rekomendację z rejestru.

**Rozstrzygnięte, nie wracaj do nich bez wyraźnej prośby:** renderer to GLES2 + Pixman ze smithaya,
nie `wgpu` (D-001) · karty i okna są całkowicie rozłączne, `Tab` nie zna okien (D-003) ·
**okna kafelkują się, nie nakładają i nie są przesuwane myszą** (D-025) ·
nazwy: marka GOST OS, technicznie `gostui` (D-014) · licencja GPL-3.0 ·
**podstawą systemu na PC i RPi jest Debian, Arch odrzucony** (D-034) ·
**budżet pamięci liczony jako prywatna, nie RSS** (D-038) · **cache bez limitu = wyciek** (D-039) ·
**poza kompozytorem nic nie działa stale, XWayland dopiero przy kliencie X11** (D-040).

### Model okien to kafelkowanie (D-025) — trzy pułapki
1. **Kafelkowanie ≠ wszystkie okna widoczne.** Obowiązuje limit kafelków (2 na wąskim ekranie),
   reszta okien czeka na dolnym pasku. Nowe okno ponad limit zastępuje kafelek z fokusem.
2. **Dialogi, okna wyboru pliku i popupy NIE są kafelkowane** — pływają wyśrodkowane nad rodzicem.
   Kafelkowanie okna „Zapisz jako" to najczęstszy sposób, w jaki kafelkujący kompozytor staje się
   nieużywalny.
3. **Respektuj `set_min_size`** klienta — aplikacji, która nie mieści się w kafelku, nie kafelkujemy.

Podział wzdłuż dłuższej osi ekranu (pion → kafelki jeden nad drugim). Suwak podziału przeciągany
jest dozwolony — to nie to samo, co swobodna zmiana rozmiaru okna.

**Nadal otwarte:** D-004 (VFS vs. `sshfs`, blokuje M6), D-012 (zakres protokołów, domykany
przyrostowo), dystrybucja **na telefonie** (pmOS vs. Mobian — strona PC-towa zamknięta przez D-034).

### Dwie granice, nie jedna (D-016 i D-037)
D-016 mówi, że kod logiki nie wie, że pod spodem jest smithay. **D-037 dokłada drugą oś: kod UI
nie wie, że pod spodem jest Debian.** Operacje na pakietach i usługach idą przez trait
(`gostui-system`, jeszcze nie istnieje) z implementacjami `apt + systemd` i `apk + OpenRC` —
telefon prawdopodobnie pojedzie na Alpine, więc dwa środowiska są pewne, nie hipotetyczne.
**Nigdy `systemctl` ani `apt` wołane wprost z panelu sterowania.**

### `Output` nigdy nie zakłada fizycznego ekranu (D-035)
Sesja zdalnego pulpitu to wyjście bez sprzętu (headless) z wstrzykiwanym wejściem — wymagania
identyczne jak dla stacji dokującej z D-026. Jeśli w core przemyci się założenie „wyjście =
podłączony monitor", zdalny pulpit będzie przepisywaniem, a nie dopisaniem. Testowalne dziś:
model wyjść musi pozwalać utworzyć wyjście bez odpowiednika sprzętowego i skafelkować je.

### Cel: zrobić z telefonu komputer (D-020, D-022, D-023, D-024)
To **nie jest** projekt „shella mobilnego". Celem jest telefon używany jak komputer: normalne
programy desktopowe, mysz i klawiatura Bluetooth, zapisywalny system z menedżerem pakietów.
Kolejność platform: PC → RPi → telefon.

**Trzy ścieżki wejścia, każda do czego innego:**
| Co obsługujesz | Czym |
|---|---|
| własny UI GostUI | dotyk bezpośredni — cele ≥ 48 px, przesunięcia, długie przytrzymanie |
| obce aplikacje desktopowe | **tryb wskaźnika** — wirtualny gładzik, ruch względny, kompozytor rysuje kursor |
| mysz/klawiatura Bluetooth | normalne `wl_pointer` / `wl_keyboard` |

`wl_touch` musi być **osobną ścieżką**, nie przemianowanym `wl_pointer` — właśnie po to, żeby tryb
wskaźnika (D-022) dał się czysto zaimplementować obok dotyku bezpośredniego.

**Docelowy scenariusz to stacja dokująca (D-026):** telefon musi mieć wyjście obrazu (DisplayPort
alt mode → praktycznie rodzina SDM845: OnePlus 6/6T, Pocophone F1 — porty w pmaports `community`).
Moto G8 zostaje sprzętem do testów dotyku, nie do dokowania.

**Musi wejść od razu, bo dorabianie później to przepisywanie:**
- **wyjścia w kolekcji**, skala **i transformacja (obrót)** per wyjście — od M1, nawet gdy wyjście
  jest jedno. Kafelkowanie i limit kafelków liczone **per wyjście**: ekran telefonu pionowo
  i monitor poziomo w tej samej sesji;
- **przeżycie odłączenia wyjścia** — okna stojące na znikającym wyjściu wracają na pozostałe.
  To najczęstsze miejsce paniki kompozytora, a przy dokowaniu zdarza się codziennie;
- żadna funkcja dostępna wyłącznie po najechaniu; każdy skrót i każde menu kontekstowe ma mieć
  odpowiednik dotykowy;
- przy przesuwaniu palcem karta podąża za palcem 1:1 (D-021) — to nie animacja, tylko informacja
  zwrotna; animacje dekoracyjne pozostają zakazane.

**XWayland (M5) to nie „bonus na Warstwę 2", tylko warunek podstawowego zastosowania** — skoro
użytkownik instaluje normalne programy, większość będzie aplikacjami X11.

Aparat, modem/SIM i GPS są świadomie wyłączone; **Bluetooth jest krytyczny** (bez niego nie ma
myszy ani klawiatury, czyli nie ma komputera).

### Granica, której nie wolno przekroczyć (D-016)
Cała logika w crate'ach bez zależności od `smithay` / `wayland-*`: model kart, konfiguracja, parser
`.desktop`, VFS, obliczenia layoutu, mapa skrótów. Kompozytor tylko tłumaczy zdarzenia protokołu
na wywołania core i rysuje jego stan.

**Praktyczny test:** jeśli piszesz logikę, do której przetestowania potrzebny jest działający
kompozytor — jest w złym miejscu. Przenieś ją do core i przetestuj `cargo test`.

### Renderowanie
Specyfikacja wymaga **zera renderowania w spoczynku**. Nigdy nie pisz bezwarunkowej pętli render loop.
Renderowanie wyłącznie w reakcji na zdarzenie (wejście, damage klienta, zmiana stanu) i wyłącznie
uszkodzonych regionów. To wymaganie architektoniczne, nie optymalizacja — złamane raz, przenika wszędzie.

### Lekkość na starym sprzęcie (D-027)
Stary PC jest **głównym celem wdrożeniowym**, nie efektem ubocznym. Trzy konsekwencje:
- **Ścieżka Pixman jest równorzędna, nie awaryjna** — na maszynie bez GPU bije GLES2 na `llvmpipe`.
  Obie muszą działać na każdym etapie.
- **Budżet mierzymy na pamięci prywatnej, nie na RSS (D-038).** RSS w backendzie zagnieżdżonym
  jest w dwóch trzecich współdzieloną Mesą (`libLLVM` 46,6 MB!), której na docelowej ścieżce
  DRM + Pixman nie będzie wcale — egzekwowanie go daje fałszywy alarm dziś i fałszywy spokój
  jutro. Progi: **Pixman ≤ 50 MB, GLES2 ≤ 70 MB pamięci prywatnej**. Zmierzone 2026-08-01
  z podłączonym `foot`: 31,9 MB (Pixman) i 27,5 MB (GLES2) prywatnych, przy RSS 101/97 MB.
  **Testu psującego build nadal nie ma** — ale wiadomo już, co ma mierzyć. Jak rozbić pomiar:
  `docs/01-strategia-dev-test.md` §3.7 D.
- **Żaden cache nie rośnie bez ograniczenia (D-039).** Powłoka chodzi tygodniami, więc cache bez
  limitu jest wyciekiem — zmierzone: cache tekstu kluczowany całym napisem rósł o **5,3 MB
  na dobę** przez sam zegar. Każdy nowy cache (ikony, miniatury, wyniki wyszukiwania) ma mieć
  limit **i test** w kształcie „N różnych kluczy zostawia ≤ M wpisów". Pojedynczy pomiar pamięci
  tego nie złapie — po minucie pracy wszystko wygląda dobrze.
- **Lekkość mierz pod ograniczeniami, nigdy na tej stacji.** `systemd-run --user --scope
  -p MemoryMax=512M -p CPUQuota=100%` (cgroup v2 dostępne) i `LIBGL_ALWAYS_SOFTWARE=1`.
  Szczegóły: `docs/01-strategia-dev-test.md` §3.7.

Nie obiecuj wsparcia 32-bitowego — Debian trixie nie ma dla `i386` instalatora ani jądra.

### Odporność
- W obsłudze żądań klienta **żadnego `unwrap()` / `expect()`** na danych pochodzących od klienta.
  Błąd protokołu musi zabić klienta, nie kompozytor.
- Zapis konfiguracji zawsze atomowo (plik tymczasowy + `rename`).
- Awaria kompozytora = utrata wszystkich aplikacji użytkownika. Traktuj panikę jako błąd krytyczny.

### Ikony, tekst, skalowanie
- Tekst przez `cosmic-text` (nie sam rasteryzer — patrz D-005).
- Ikony przez wyszukiwanie zgodne z freedesktop Icon Theme Spec + rendering SVG + cache per rozmiar.
- Layout **w jednostkach logicznych**, skala per wyjście mnożona dopiero przy rasteryzacji (D-011),
  nawet gdy w v1 skala jest zawsze 1.0.

### Nie wynajduj tego, co jest wystandaryzowane
Projekt ma być lekki, ale ma współpracować z systemem. Używaj istniejących specyfikacji freedesktop:
`.desktop` (skróty, z obsługą kodów pól `Exec`), `~/.local/share/Trash` (kosz),
`mimeapps.list` + `shared-mime-info` (skojarzenia plików), `org.freedesktop.Notifications`
(powiadomienia), `StatusNotifierItem` (tray), Icon Theme Spec (ikony).
Własny format = niekompatybilność z resztą systemu użytkownika.

### Zakres
Warstwa 2 specyfikacji (przeglądarka, RDP, Moonlight, emulator C64) to **obce aplikacje do uruchomienia**,
nie funkcje do napisania. Ich koszt to XWayland + poprawny zestaw protokołów. Nie proponuj pisania
własnej przeglądarki ani emulacji CPU 6510 — użyj VICE.

## Uruchamianie i testowanie

**Domyślny tryb pracy: kompozytor zagnieżdżony jako okno w sesji XFCE użytkownika** (backend `winit`).
Awaria kosztuje wtedy jedno okno, nie całą sesję z otwartymi programami.

```bash
# terminal 1 — kompozytor. Wypisuje nazwę gniazda przy starcie.
RUST_BACKTRACE=1 cargo run -p gostui-compositor -- --backend winit
# terminal 2 — klient testowy i diagnostyka
WAYLAND_DISPLAY=wayland-gostui wayland-info   # lista globali
WAYLAND_DISPLAY=wayland-gostui foot           # startuje; okna jeszcze nie widać (krok 3 M2)
```

**Skróty powłoki kolidują z XFCE i to nie jest usterka.** `xfwm4` trzyma `Super+Tab`
i `Super`+strzałki, a `xfsettingsd` `Super+F` — klawisz złapany przez sesję-gospodarza nie dociera
do zagnieżdżonego okna w ogóle, więc `Super+Tab` wygląda na zepsuty, a `Super+F` zamiast pełnego
ekranu otwiera menedżer plików. Na gołym metalu (M4) problem nie istnieje. Do pracy w oknie:

```bash
scripts/xfce-zwolnij-skroty.sh              # zwalnia sześć kolidujących skrótów
scripts/xfce-zwolnij-skroty.sh --przywroc   # oddaje dokładnie to, co było
```

To zmiana w **sesji użytkownika**, nie w projekcie — dlatego jest skryptem uruchamianym świadomie,
a nie czymś, co dzieje się przy starcie kompozytora.

`WAYLAND_DISPLAY` ustawiaj **klientowi, nie kompozytorowi** — kompozytor jest gościem w sesji
XFCE i sam nie łączy się ze swoim gniazdem. Gniazdo nazywa się `wayland-gostui`; jeśli jest
zajęte (druga instancja), bierzemy automatyczne i piszemy jakie.

**Nie proponuj uruchamiania na tty (DRM/KMS) w trakcie normalnej iteracji.** Tylko przy domykaniu
etapu, zawsze z zabezpieczeniami z `docs/01-strategia-dev-test.md` §2.2 Tier 2 (`timeout 120`,
włączone SysRq, dostępny SSH, logi do pliku).

Wydajność mierz **wyłącznie na `--release`** — build debug renderera softwarowego daje fałszywy alarm.

## Środowisko stacji (2026-07-30)

Ubuntu 24.04, XFCE na X11, AMD Vega 11 (Mesa/RADV), 29 GiB RAM, KVM dostępny, jeden monitor HDMI
1920×1080, Rust 1.96. **Brak `/sys/class/backlight`** — kontrolka jasności nie ma na czym działać,
musi się ukrywać. Brak baterii — sekcja baterii też musi się ukrywać.
Pełna inwentaryzacja i lista brakujących pakietów: `docs/01-strategia-dev-test.md` §1.

## Higiena repozytorium

### Zasada czystego repozytorium (obowiązkowa, sprawdzana maszynowo)

**Repozytorium zawiera projekt i nic poza projektem.** To jest repozytorium publiczne: wszystko,
co tu wejdzie, wchodzi na zawsze — historia git nie zapomina, a usunięcie pliku w kolejnym
commicie niczego nie usuwa. Cztery rzeczy, których **nigdy** nie commitujemy:

1. **Stan lokalny narzędzi.** Pliki konfiguracji edytora, asystentów, powłoki, ich pliki
   tymczasowe i cokolwiek, co opisuje **czym** i **gdzie** pracujesz, a nie **co** zbudowałeś.
   Ścieżki lokalne (`/home/...`, `/tmp/...`) nie mają wstępu do repozytorium — ani w plikach,
   ani w komunikatach commitów.
2. **Dane osobowe.** Prywatne adresy e-mail, identyfikatory kont w usługach zewnętrznych,
   nazwy hostów, numery seryjne. Commity idą wyłącznie z adresu `@users.noreply.github.com`.
3. **Metadane plików binarnych.** Grafiki niosą w EXIF/XMP identyfikatory konta narzędzia,
   w którym powstały. Obrazy wchodzą do repozytorium **przepisane bez metadanych**.
4. **Nazwy narzędzi w treści projektu.** Kod i dokumentacja opisują **projekt**, nie warsztat.
   Odwołania w komentarzach kierują do dokumentów (`docs/02-decyzje.md`, D-025), nigdy
   do pliku nazwanego po dostawcy narzędzia.

**Dlaczego to jest zasada, a nie preferencja:** jednorazowe niedopatrzenie jest trwałe. Skan
z 2026-08-01 znalazł w historii trzy commity z plikami tymczasowymi asystenta, zawierającymi
pełne ścieżki lokalne — dopisane odruchowo przez `git add -A`. Żaden przegląd kodu tego nie
wyłapał, bo nikt nie czyta `git add -A`.

**Jak to jest egzekwowane — dwie warstwy, żadna nie polega na czyjejś pamięci:**

| Warstwa | Kiedy działa | Co robi |
|---|---|---|
| `scripts/higiena.sh` przez hak `pre-commit` | przy `git commit`, lokalnie | **odrzuca commit**, zanim cokolwiek powstanie |
| zadanie `higiena` w CI | przy każdym pushu | **wywala build** — siatka bezpieczeństwa, gdy hak nie był zainstalowany |

Ten sam skrypt w obu miejscach, więc nie da się przejść lokalnie i polec w CI z innego powodu.

**Zainstaluj hak przed pierwszym commitem na nowej maszynie** — katalog `.git/hooks/` nie
podróżuje z klonem, więc świeży klon jest niepilnowany do momentu:

```bash
scripts/zainstaluj-haki.sh
```

**Gdy hak albo CI cokolwiek zgłosi: usuń przyczynę, nie ostrzeżenie.** `git commit --no-verify`
w tym repozytorium jest równoznaczne z wypuszczeniem danych na zewnątrz.

### Praca na gałęziach (3/3 — pełna zasada)

**Na `main` nic nie powstaje. Powstaje na gałęzi i wchodzi przez pull request z zielonym CI.**

Trzy powody, każdy sam wystarczy:

1. **Krok etapu może wyjść źle i trzeba go wycofać.** M2 jest w `docs/01` oznaczone jako
   ⚠️ punkt weryfikacji ryzyka — to nie ozdobnik, tylko zapowiedź, że któryś krok może okazać się
   pomyłką. Porzucenie gałęzi kosztuje nic. Cofanie `main` kosztuje historię, **która jest już
   publiczna i której git nie zapomina** — dokładnie ta sama arytmetyka, co w zasadzie czystego
   repozytorium wyżej.
2. **Werdykt CI ma przychodzić przed `main`, nie po.** Workflow startuje i na `push` do `main`,
   i na `pull_request`. Bez gałęzi dowiadujesz się, że build jest zepsuty, gdy jest już zepsuty
   na gałęzi głównej.
3. **Recenzja ma gdzie mieszkać.** W tym projekcie propozycja techniczna idzie przed kodem
   (patrz `§ Zanim zaproponujesz rozwiązanie techniczne`). Opis PR-a jest jedynym miejscem, gdzie
   ta recenzja zostaje na stałe — rozmowa znika, opis PR-a zostaje przy commitach na zawsze.

Rytm, jedna gałąź na krok etapu:

```bash
git switch -c m2/krok-5-dekoracje         # nazwa: <etap>/<krok-lub-temat>
# ... praca, commity jak zwykle — hak higieny działa tak samo
git push -u origin m2/krok-5-dekoracje
gh pr create --fill                       # CI rusza na PR
gh pr checks --watch                      # zielone albo nie ma merge'a
gh pr merge --rebase --delete-branch       # historia zostaje liniowa, commity osobno
```

**`--rebase`, nie `--squash`:** commity w tym repozytorium są pisane po to, żeby je czytać
pojedynczo. Zgniecenie kroku etapu w jeden commit kasuje właśnie tę wartość.

**Egzekwowane, nie deklarowane.** `main` na GitHubie jest chroniony: push wprost odrzucany,
merge tylko przez PR z przechodzącym CI. Zasada, której nikt nie egzekwuje, jest życzeniem —
to zdanie z początku tego dokumentu dotyczy także tej zasady.

**Jedyny wyjątek:** poprawka samej konfiguracji CI, która uniemożliwia przejście CI (kurczak
i jajko). Wtedy PR z opisem, dlaczego nie dało się inaczej.

### Pozostałe

- **Commit ma jednego autora i żadnych stopek** poza treścią. Komunikat opisuje, co się zmieniło
  i dlaczego — nie czym to napisano.
- Zależności systemowe (pakiety `-dev`) dopisuj na bieżąco do `docs/zaleznosci.md` — odtwarzanie
  tej listy przy pakowaniu jest niepotrzebnie żmudne.
- Nowa decyzja architektoniczna → nowy wpis w `docs/02-decyzje.md`. Zmiana decyzji → nowy wpis
  ze statusem poprzedniej `ZASTĄPIONA`, bez usuwania historii.
