# Strategia developmentu i testowania — stacja robocza

Dokument opisuje, **jak konkretnie budować i testować GostUI na tej maszynie**, oraz w jakiej
kolejności, z jawnym kryterium weryfikacji dla każdego etapu.

---

## 1. Inwentaryzacja stacji (stan na 2026-07-30)

| Element | Wartość | Znaczenie dla projektu |
|---|---|---|
| System | Ubuntu 24.04.4 LTS (noble) | Specyfikacja zakłada Debiana — patrz 2.1 |
| Sesja | XFCE na **X11** | Idealne: kompozytor testujemy zagnieżdżony w oknie, awaria nie zabija sesji |
| GPU | AMD Vega 11 (APU), Mesa RADV | Vulkan działa, GLES działa. Sterownik `amdgpu` → `atomic modesetting` dostępne |
| RAM | 29 GiB | Wystarczy na maszynę wirtualną z Debianem obok pracy |
| KVM | `/dev/kvm` obecny | Wirtualizacja sprzętowa dostępna |
| Monitor | 1 × HDMI-A-1, 1920×1080 | v1 celuje w jedno wyjście (patrz przegląd, 4.9) |
| Backlight | **brak** `/sys/class/backlight` | Suwak jasności nie ma na czym działać (przegląd, 4.10) |
| Rust | 1.96.0 (cargo 1.96.0) | Aktualny, wystarczający |
| Grupy użytkownika | `video`, `render`, `sudo`, `docker`, `input` — **brak** | `video`+`render` wystarczają do DRM/KMS; `input` przez `seatd`/logind |
| Wolne VT | tty2, tty3, tty4 | Do testów na goliźnie (Tier 2) |
| Polkit | `polkitd` + agent MATE działa | Wzór do naśladowania w sesji GostUI |

### Brakujące pakiety deweloperskie

Zainstalowane: `libwayland-dev`, `libxkbcommon-dev`, `libegl1-mesa-dev`, `libdbus-1-dev`.
Brakuje (potrzebne do backendu DRM/KMS i wejścia):

```
sudo apt install libinput-dev libudev-dev libseat-dev libgbm-dev libdrm-dev libsystemd-dev \
                 seatd weston wayland-utils \
                 qemu-system-x86 qemu-utils ovmf virt-manager \
                 xdg-desktop-portal xdg-desktop-portal-gtk \
                 foot   # minimalny terminal wayland-native — nasz podstawowy klient testowy
```

**Stan na 2026-07-30 — zweryfikowany:** dziesięć pakietów z pierwszej linii jest **zainstalowanych**
(`libinput-dev`, `libudev-dev`, `libseat-dev`, `libgbm-dev`, `libdrm-dev`, `libsystemd-dev`,
`seatd`, `weston`, `wayland-utils`, `foot`). Do zainstalowania zostały narzędzia pomiarowe i VM —
potrzebne dopiero od M4 i przy egzekwowaniu progów z D-027, nie do startu M0:

```
sudo apt install qemu-system-x86 qemu-utils ovmf \
                 valgrind heaptrack hyperfine \
                 xdg-desktop-portal xdg-desktop-portal-gtk
```

Obecne i istotne dla D-027: **cgroup v2** (`cgroup2fs`) → `systemd-run` z `MemoryMax`/`CPUQuota`
działa, oraz sterowniki software'owe Mesy (`swrast_dri.so`, `kms_swrast_dri.so`) → da się udawać
komputer bez GPU. Szczegóły użycia: §3.7.

Uwagi:
- `weston` instalujemy **nie** jako środowisko, a po pierwsze dla klientów `weston-simple-shm`,
  `weston-simple-egl`, `weston-terminal` (najmniejsze możliwe klienty testowe — jeśli one nie
  działają, problem jest w kompozytorze, nie w toolkicie), po drugie jako implementację
  referencyjną do porównania zachowania protokołu.
- `wayland-utils` daje `wayland-info` — wypisuje globalne obiekty wystawiane przez kompozytor.
  To pierwsze narzędzie diagnostyczne przy każdym „aplikacja się nie uruchamia".
- `seatd` (demon) pozwala uruchomić kompozytor na tty bez uprawnień roota.
  Po instalacji: `sudo systemctl enable --now seatd` i dodanie użytkownika do grupy `_seatd`.

---

## 2. Środowisko: gdzie właściwie budować

### 2.1 Ubuntu 24.04 vs. Debian ze specyfikacji

Specyfikacja mówi „Debian minimalny, bez DE". To słuszny **cel wdrożeniowy**, ale zły **warsztat**.
Rekomendacja: rozdzielić te dwie rzeczy.

- **Buduj i pisz kod tutaj** (Ubuntu + XFCE). Masz działający system, edytor, przeglądarkę,
  dokumentację i debugger. Ubuntu 24.04 i Debian 13 mają zbliżone wersje `libwayland`, `libinput`
  i Mesa — kod przenosi się bez zmian.
- **Weryfikuj na Debianie minimalnym w maszynie wirtualnej** (Tier 3). To tam sprawdzasz założenie
  „działa bez żadnego DE", plik sesji, greetd, uprawnienia i listę zależności pakietu.

Nie ma powodu instalować Debiana na goliźnie tej maszyny. Utrata narzędzi pracy > zysk z „czystego" systemu.

### 2.2 Cztery poziomy uruchamiania — od najtańszego

To jest kręgosłup całej strategii testowania. Prawie cała praca dzieje się na Tier 0.

#### Tier 0 — logika bez grafiki (~60% pracy)
Crate `gostui-core`: model kart, parser `.desktop`, konfiguracja, VFS, ścieżki kosza, sortowanie,
mapa skrótów klawiszowych, maszyna stanów fokusu. **Zero zależności od smithaya, wgpu, wayland.**
Uruchamianie: `cargo test`. Czas cyklu: sekundy. Debugger działa normalnie.

Reguła architektoniczna, która to umożliwia: kompozytor jest cienką warstwą tłumaczącą zdarzenia
Waylanda na wywołania `gostui-core` i rysującą jego stan. Logika nigdy nie dotyka protokołu.
Jeśli ta granica się rozmyje, testowalność projektu spada do zera — to najważniejsza decyzja
techniczna w całym planie.

#### Tier 1 — kompozytor zagnieżdżony w oknie XFCE (~30% pracy)
Backend `winit` smithaya: kompozytor uruchamia się jako **zwykłe okno X11** w bieżącej sesji XFCE.
Widzisz paski, slider, karty; klienci Wayland (`foot`, `weston-terminal`, GTK, Qt) łączą się
do niego normalnie.

```bash
# w oknie 1: kompozytor
WAYLAND_DISPLAY=wayland-gostui cargo run -p gostui-compositor -- --backend winit
# w oknie 2: klient testowy
WAYLAND_DISPLAY=wayland-gostui foot
WAYLAND_DISPLAY=wayland-gostui wayland-info | less
```

Dlaczego to jest domyślny tryb pracy:
- awaria kompozytora zamyka **jedno okno**, sesja XFCE żyje, edytor i przeglądarka nie giną,
- `RUST_BACKTRACE=1`, `gdb`, `rust-lldb` działają bez ceregieli,
- przeładowanie po zmianie kodu to `Ctrl+C` i `cargo run`,
- da się nagrywać ekran i zrzuty zwykłymi narzędziami XFCE.

#### Tier 2 — DRM/KMS na goliźnie, na wolnym VT (~5% pracy, ale niezbędne)
Backend `udev`+`drm`+`libinput` przez `seatd`. To jedyny sposób sprawdzenia realnej ścieżki:
atomic modesetting, `linux-dmabuf`, damage tracking, `libinput` (rzeczywista mysz i klawiatura),
przełączanie VT, tryb uśpienia i wybudzenie.

```bash
# Ctrl+Alt+F3 → logowanie tekstowe na tty3
cd ~/projects/GostOs && timeout 120 ./target/debug/gostui-compositor --backend udev
```

**Zasady bezpieczeństwa (obowiązkowe, zanim pierwszy raz odpalisz na tty):**
1. Zawsze przez `timeout 120` — jeśli kompozytor zablokuje wejście, sam się zabije po 2 minutach.
2. Włączone klawisze SysRq jako awaryjne wyjście:
   `sudo sysctl -w kernel.sysrq=1` (trwale: `kernel.sysrq = 1` w `/etc/sysctl.d/99-sysrq.conf`).
   Ratunek: `Alt+SysRq+R` (odbierz klawiaturę trybowi raw), potem `Alt+SysRq+E`, `I`, `S`, `U`, `B`.
3. Drugi kanał dostępu: `sudo systemctl enable --now ssh` i logowanie z telefonu lub laptopa —
   pozwala zabić proces bez restartu maszyny.
4. Logi do pliku, nie na ekran: `--backend udev 2> ~/gostui-tty.log` (ekran właśnie przejmuje kompozytor).

#### Tier 3 — maszyna wirtualna z Debianem minimalnym (weryfikacja wdrożenia)
QEMU/KVM, Debian 13 bez DE. Tutaj i tylko tutaj testujesz: instalację z pakietu, plik sesji,
`greetd`, listę zależności, uruchomienie na czystym systemie, oraz — dzięki `virtio-gpu` bez
akceleracji — **renderer softwarowy** (fallback dla RPi3, patrz przegląd 2.2).

```bash
qemu-system-x86_64 -enable-kvm -m 4096 -smp 4 \
  -device virtio-gpu-gl -display gtk,gl=on \
  -device virtio-keyboard,-device virtio-tablet \
  -drive file=~/vm/debian-gostui.qcow2,if=virtio
```
Uwaga: `virtio-gpu-gl` + `-display gtk,gl=on` daje akcelerację (Virgl); bez `-gl` dostajesz
ścieżkę czysto programową — oba warianty są potrzebne, jako dwa różne testy.

### 2.3 Wniosek o kolejności backendów

Backend musi być **abstrakcją od pierwszej linii kompozytora** (`winit` | `udev` | `headless`),
bo Tier 1 i Tier 2 to ten sam kod z inną warstwą wejścia/wyjścia. Smithay to zakłada i ma
przykłady (`anvil`) pokazujące dokładnie taki podział — warto go skopiować, a nie wymyślać.

---

## 3. Strategia testowania

### 3.1 Piramida — co czym testować

| Poziom | Zakres | Narzędzie | Gdzie działa |
|---|---|---|---|
| Jednostkowe | model kart, parser `.desktop`, config serde (round-trip), ścieżki kosza, sortowanie, dopasowanie w wyszukiwaniu, layout (obliczenia stref, skrawków, kafelków) | `cargo test` | Tier 0, CI |
| Property-based | parser `.desktop` i ścieżek — `proptest`: żadne wejście nie może wywołać paniki | `proptest` | Tier 0, CI |
| Migawkowe (snapshot) | serializacja konfiguracji, wynik obliczeń layoutu jako tekst | `insta` | Tier 0, CI |
| Zrzuty ekranu (golden) | render pasków, slidera, menu — porównanie PNG z wzorcem | renderer Pixman + `image` | Tier 0/CI (headless!) |
| Zgodność protokołu | czy klienci działają | `wayland-info`, `weston-simple-*`, `foot`, GTK4-demo, Qt6, Firefox | Tier 1 |
| Odporność | złośliwie sformułowane żądania klienta nie zabijają kompozytora | własny klient-fuzzer na `wayland-client` | Tier 1 |
| Sprzętowe | modesetting, dmabuf, libinput, VT, uśpienie | ręcznie | Tier 2 |
| Wdrożeniowe | czysty system, pakiet, sesja, greeter | ręcznie | Tier 3 |
| Wydajnościowe | progi z przeglądu 6.2 | licznik klatek + `/proc`, `hyperfine` | Tier 1 + Tier 2 |

### 3.2 Testy zrzutów ekranu bez GPU — kluczowy trik

Renderer Pixman (CPU) rysuje do bufora w pamięci i jest **deterministyczny** — ten sam stan daje
bit w bit ten sam obraz. To pozwala na coś, co w projektach GUI zwykle jest nieosiągalne:
**testy layoutu i wyglądu uruchamiane w CI, bez ekranu i bez karty graficznej.**

Schemat: ustaw stan (`gostui-core` z 5 kartami, karta 2 aktywna, tryb „tylko ikony") → wyrenderuj
klatkę do bufora → zapisz PNG → porównaj z wzorcem w repo (`tests/golden/*.png`), z tolerancją
na pojedyncze piksele antyaliasingu tekstu. Regresja w layoucie pasków przestaje wymagać zauważenia
gołym okiem.

To drugi argument (po przeglądzie 2.1) za tym, żeby renderer Pixman był obecny od początku,
a nie tylko GLES.

### 3.3 Klienci testowi — kolejność rosnącej trudności

Kompozytor „działa" etapami. Kolejność, w jakiej powinno się zdobywać klientów:

1. `weston-info` / `wayland-info` — wypisuje globalne obiekty. Nie rysuje nic. Pierwszy cel.
2. `weston-simple-shm` — kilkaset linii C, bufor `wl_shm`, bez EGL. Jeśli to nie działa,
   problem jest w rdzeniu.
3. `weston-simple-egl` — ścieżka dmabuf/GPU.
4. `foot` — prawdziwy terminal, wayland-native, minimalne zależności. Tu wchodzi klawiatura,
   `xkbcommon`, `xdg-decoration`, schowek.
5. **GTK4** (`gtk4-demo`) i **Qt6** — dwa najważniejsze toolkity; wykrywają braki w skalowaniu,
   dekoracjach, popupach, DnD, wprowadzaniu tekstu.
6. **Firefox** (wayland-native) — ostateczny test na popupy, wielookienność, wideo, schowek, portale.
7. `wleird` (jeśli dostępny) — zestaw klientów celowo zachowujących się dziwnie; łapie błędy
   obsługi protokołu, których żaden normalny klient nie wywoła.
8. **Aplikacje X11 przez XWayland** — `xterm`, `xeyes`, potem VICE (C64) jako realny cel z Warstwy 2.

Zasada: **nowy protokół nie jest zaimplementowany, dopóki nie ma klienta, który to udowadnia.**

### 3.4 Wejście: nie automatyzować na poziomie sprzętu

Symulowanie kliknięć w Waylandzie z zewnątrz jest celowo utrudnione (brak `XTEST`). Zamiast walczyć:

- **Interakcje testować na poziomie modelu** — podawać syntetyczne zdarzenia wprost do
  `gostui-core` (`handle_key(Super+Right)` → sprawdzić, że aktywna karta wzrosła i stan się zapisał).
  Pokrywa 95% logiki interakcji, działa w CI.
- Dla Tier 2, jeśli naprawdę potrzeba realnego wejścia: wirtualne urządzenia `uinput`
  (`python3-evdev`) — `libinput` widzi je jak fizyczną mysz. Stosować oszczędnie, do testów
  regresji obsługi urządzeń, nie do testów UI.

### 3.5 Pomiar „zero renderowania w spoczynku"

Cel ze specyfikacji jest wprost testowalny, ale wymaga instrumentacji od początku:
licznik wyrenderowanych klatek eksponowany przez zmienną środowiskową `GOSTUI_STATS=1`.

**Zrobione 2026-08-01 (krok 6 M1) — z jednym odstępstwem od pierwotnego zapisu.**
Ten paragraf mówił wcześniej „wypis do logu **co sekundę**". To był błąd: timer budzący proces
raz na sekundę to raz na sekundę wybudzenie. Na telefonie kosztuje baterię, na starym procesorze
wentylator (D-027), a w `top` widać by było CPU spalone przez samą instrumentację — pomiar
zaburzający to, co mierzy. Zamiast tego:

- **linia na każdą narysowaną klatkę:** numer, **powód** (`initial` / `resized` / `redraw` /
  `clock` / `client`), czas renderu, przerwa od poprzedniej klatki. Zero klatek = zero linii, zero
  dodatkowych wybudzeń;
- **raport przy zamknięciu:** klatki, min/średnia/max/suma czasu renderu, **udział renderowania
  w czasie pracy procesu**, rozbicie na powody, najdłuższa przerwa między klatkami.

Powód klatki jest tu ważniejszy od samego licznika: „47 klatek" to liczba, a „47 klatek, wszystkie
`redraw`" nazywa błąd.

**Powód musi być prawdziwy, a przez trzy dni nie był (naprawione 2026-08-04).** `request_redraw`
ustawiał samą flagę, a pętla rysowała bezwarunkowo z `Cause::Client` — więc **każda** klatka
zamówiona przez powłokę (klik w kartę, skrót klawiszowy) meldowała się jako klatka od klienta.
Prawdę mówiły tylko ścieżki wołające `draw` wprost: zegar i zmiana fokusu. Wykryte na żywej
powłoce: sesja, w której nie było ani jednego klienta, zaraportowała `client 61`. Dziś
`request_redraw` przyjmuje powód, a etykietę zatrzymuje **pierwszy proszący w danym przebiegu
pętli** — to on nas obudził. Pomiary z 2026-08-01 zostają w mocy (tamte klatki naprawdę były
od klienta), ale **rozbicie na powody z tamtych przebiegów nie jest dowodem na nic**.

**Kryterium jako komenda, nie jak oglądanie.** `--idle-test <sekundy>` trzyma okno zadany czas
i kończy się **kodem wyjścia**:

```bash
GOSTUI_STATS=1 cargo run --release -p gostui-compositor -- \
    --backend winit --renderer pixman --idle-test 10
```

Trzy rzeczy, które w tym teście musiały zostać uwzględnione, bo bez nich dawał fałszywy alarm:

1. **Otwieranie okna to nie spoczynek.** XFCE przy mapowaniu okna przysyła `Resized` + `Redraw`
   w ciągu ~10 ms od startu. Pomiar zaczyna się dopiero **sekundę po starcie**, inaczej mierzyłby
   zarządzanie oknami X11, a nie naszą politykę rysowania.
2. **Klatka zegara nie jest usterką.** Okno pomiarowe dłuższe niż minuta zawiera przerysowanie
   zegara — to krok 5 działający poprawnie, nie pętla. Test liczy osobno klatki od zegara
   i klatki **bez powodu**; niezerowe są tylko te drugie.
3. **Klatka od klienta też nie jest usterką (dopisane 2026-08-01, M2 krok 2.)** Aplikacja, która
   otwiera okno, zmienia tytuł albo się zamyka, zmienia to, co jest na ekranie — powłoka **ma**
   wtedy narysować klatkę. Wygląda to na furtkę i nią nie jest: klatki od klientów są liczone
   **osobno** (`od klientów: n`), więc klient budzący nas za często dalej widać, zamiast rozmyć
   go w sumie. Zmierzone z podłączonym `foot`: 3 klatki na 12 s, **0 bez powodu**.

**Zmierzone 2026-08-01 na tej stacji** (`--release`, okno 1360×850, nietykane):

| Ścieżka | Okno pomiarowe | Klatki bez powodu | Udział renderowania | Czas klatki |
|---|---|---|---|---|
| GLES2 | 8 s | **0** | 0,19% | 0,85–15,3 ms |
| Pixman | 12 s | **0** (1 od zegara) | 0,29% | 3,9–21,0 ms |

Uzupełniająco `top -p $(pgrep gostui)` (CPU < 1%) i `radeontop` (aktywność GPU ~0%).

Jeśli licznik rośnie w spoczynku, znaczy że gdzieś jest bezwarunkowa pętla renderowania —
i to jest błąd architektoniczny do naprawy natychmiast, bo później przenika wszędzie.

**Czego ten test jeszcze nie pilnuje:** damage jest w tej chwili zawsze całym oknem i raport mówi
to wprost. Rysowanie wyłącznie uszkodzonych regionów ma sens dopiero, gdy są klienci mający co
uszkadzać (M2) — do tego czasu cała powłoka i tak rysuje się ze stanu.

### 3.6 CI — co ma sens bez ekranu

**Stan: działa od 2026-07-30**, `.github/workflows/ci.yml`, trzy zadania:

| Zadanie | Co robi | Czas |
|---|---|---|
| `check` | `fmt --check`, `clippy -D warnings`, `cargo test`, build trzech backendów | ~50 s |
| `deny` | `cargo deny check` — licencje zgodne z GPL-3.0 (D-014) i podatności | ~45 s |
| `cross` | build crate'ów logiki na `aarch64-unknown-linux-gnu` (D-002, D-026) | ~30 s |

Kompilacja backendu `udev` w CI wymaga tylko nagłówków (`libseat-dev`, `libinput-dev`, `libdrm-dev`,
`libgbm-dev`), nie sprzętu — więc daje się sprawdzać bez GPU. Zadanie `cross` nie potrzebuje nawet
sysroota ARM, bo `gostui-core` nie ma żadnych zależności systemowych — granica D-016 w praktyce.

**Pułapki wyłapane przy pierwszym przebiegu** (padł; obie przyczyny dotyczyły naszych własnych
crate'ów, nie zależności):

1. `cargo-deny` porównuje identyfikatory SPDX **dokładnie**. Lista dozwolonych mówiła `GPL-3.0`,
   a crate'y deklarują `GPL-3.0-only` → wszystkie cztery odrzucone jako „licencja niedozwolona".
2. Wpisy w `[workspace.dependencies]` miały `path` bez `version`, przez co rozwiązywały się
   do wildcarda, którego `cargo-deny` nie potrafi sprawdzić. Naprawa: dopisać `version` obok
   `path`, a nie wyciszać regułę.

**Zasada, którą to ustanowiło:** *nic, czego nie uruchomiłem, nie idzie do commita.*
`Dockerfile` i `deny.toml` trafiły do repozytorium niesprawdzone i oba były zepsute.
Przed commitem: `cargo test --workspace`, `cargo deny check`, a przy zmianie w `Dockerfile`
— `docker build`.

### 3.7 Symulacja starego komputera — jak testować lekkość bez starego komputera (D-027)

Stacja robocza (Vega 11, 29 GiB RAM) jest najgorszym możliwym miejscem do oceny lekkości: wszystko
na niej działa. Dlatego lekkość mierzymy **pod narzuconymi ograniczeniami**, a nie „na oko".
Trzy mechanizmy, wszystkie dostępne na tej maszynie dziś, w kolejności rosnącego kosztu.

**A. Ograniczenie zasobów przez cgroup v2 — codziennie, koszt zerowy.**
Kernel wymusza limit RAM i CPU na procesie; nie trzeba niczego instalować (`cgroup2fs` potwierdzone).

```bash
# jeden rdzeń, 512 MB RAM — mniej więcej netbook z 2009
systemd-run --user --scope -p MemoryMax=512M -p MemorySwapMax=0 -p CPUQuota=100% \
  cargo run --release -p gostui-compositor -- --backend winit
```

Przekroczenie `MemoryMax` = zabicie procesu przez OOM killer, czyli **twardy, jednoznaczny wynik**,
nie subiektywna ocena. `CPUQuota=100%` to jeden pełny rdzeń, `50%` to pół — tak symuluje się wolny
procesor bez wolnego procesora.

**B. Wymuszenie renderowania software'owego — symulacja PC bez używalnego GPU.**
W systemie są `swrast_dri.so` i `kms_swrast_dri.so`, więc:

```bash
LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe cargo run --release ...   # GLES2 na CPU
cargo run --release -- --renderer pixman                                   # ścieżka bez GL w ogóle
```

Ścieżka Pixman jest przy tym **szybsza niż GLES2 na llvmpipe** — kompozytor 2D nie potrzebuje
potoku graficznego emulowanego na CPU. To jest właściwa ścieżka dla starego sprzętu, nie awaryjna.

**C. Maszyna wirtualna z ograniczonym sprzętem — przy domykaniu etapu.**
QEMU z `-smp 1 -m 1024 -vga std` (bez akceleracji) daje całą maszynę: wolny dysk, czas startu,
brak sterowników. To Tier 3 z §2.2, użyty do mierzenia, nie tylko do weryfikacji pakietu.

**Czego te trzy mechanizmy nie zastąpią:** przepustowości pamięci. Stary komputer przegrywa nie
liczbą megabajtów, tylko tym, jak wolno je przepisuje — pełne odświeżenie 1920×1080 w software
to ~8 MB zapisu na klatkę. **Dlatego rysowanie wyłącznie uszkodzonych regionów nie jest
optymalizacją, tylko warunkiem działania na tym sprzęcie.** Ograniczenie cgroup tego nie wykryje;
wykryje to dopiero prawdziwy stary PC albo RPi3.

**D. Rozbicie pamięci na mapowania — bo sama liczba RSS kłamie (D-038).**
Pomiar po kroku 3 M2 dał 97 MB i wyglądał na przekroczenie progu 80 MB. Rozbicie pokazało,
że dwie trzecie to **współdzielone mapowania plikowe Mesy** (`libLLVM.so` 46,6 MB, `libgallium.so`
13,0 MB), których na docelowej ścieżce DRM + Pixman nie będzie wcale, a nasze własne to 22,9 MB.

```bash
P=$(pgrep -x gostui)
# całość: RSS, PSS i to, co naprawdę nasze
awk '/^Rss:/{r+=$2} /^Pss:/{p+=$2} /^Private_Dirty:/{d+=$2} \
     END{printf "RSS %.1f · PSS %.1f · prywatne %.1f MB\n", r/1024, p/1024, d/1024}' /proc/$P/smaps
# kto zajmuje najwięcej — nazwa mapowania obok liczb
awk '/^[0-9a-f]/{n=$6; if(n=="")n="[anon]"} /^Private_Dirty:/{d[n]+=$2} /^Rss:/{r[n]+=$2} \
     END{for(k in r) if(r[k]>2048) printf "%8.1f MB RSS · %8.1f MB prywatne  %s\n", r[k]/1024, d[k]/1024, k}' \
     /proc/$P/smaps | sort -rn
```

**Egzekwujemy pamięć prywatną, raportujemy RSS** (progi: D-038). Liczba z backendu `winit` jest
orientacyjna — nawet ścieżka CPU trzyma tam kontekst EGL; rozstrzyga pomiar na DRM albo pod
`LIBGL_ALWAYS_SOFTWARE=1`.

**Pułapka, której to nie wykryje w pojedynczym pomiarze: cache, który rośnie z czasem.**
Zmierzony przypadek (D-039): cache tekstu kluczowany całym napisem rósł o **5,3 MB na dobę** przez
sam zegar. Jeden `smaps` po minucie pracy wygląda przy tym wzorowo. Dlatego limity cache'ów są
testami jednostkowymi („1440 różnych napisów zostawia ≤ 200 wpisów"), a nie obserwacją — soak
test z D-017 złapałby to po dobie, test po 40 ms.

**Wniosek strukturalny: RPi3 jest ostrzejszym progiem niż stary PC.** Cortex-A53 1,2 GHz
z 1 GB RAM jest wolniejszy od Core 2 Duo, a VideoCore IV słabsze od Intel HD. Jeśli progi
wydajności wychodzą na RPi3 (M10), na starym pececie wychodzą **za darmo** — nie trzeba osobnego
etapu ani osobnego sprzętu do zdobycia.

## 4. Harmonogram — milestone'y z kryterium weryfikacji

Każdy etap ma jawne „gotowe, gdy" testowalne na tej maszynie. Etapy są ułożone tak, by **ryzyko
techniczne rozładowywało się jak najwcześniej** (M2 i M5 to punkty, w których projekt może się
okazać trudniejszy niż założono — lepiej wiedzieć w tygodniu 3 niż w miesiącu 6).

### M0 — Szkielet i logika bez grafiki
Workspace Cargo: `gostui-core` (logika, zero I/O graficznego), `gostui-config` (TOML, zapis atomowy,
wersjonowanie schematu), `gostui-desktop-entry` (parser `.desktop` z kodami pól `Exec`),
`gostui-compositor` (na razie puste). Konfiguracja CI, `rustfmt.toml`, `clippy.toml`, `deny.toml`.

**Gotowe, gdy:** `cargo test` przechodzi, model kart obsługuje dodawanie/reorder/przypinanie
i zapis stanu per karta, parser radzi sobie z rzeczywistymi plikami z `/usr/share/applications/*.desktop`
(test: przeparsuj wszystkie obecne na tej maszynie, żaden nie może wywołać paniki).

**✅ ZAMKNIĘTE (2026-07-30).** 68 testów przechodzi, `clippy -D warnings` czysty, crate'y logiki
kompilują się na `aarch64-unknown-linux-gnu` bez sysroota ARM (bo `gostui-core` nie ma żadnych
zależności systemowych — granica D-016 spłaca się od pierwszego dnia). Do M0 doszły ponad plan:
**model wyjść** z odłączaniem (D-026) i **silnik kafelkowania** (D-025) — obie rzeczy to czysta
arytmetyka, więc należą do core, a nie do kompozytora.

Punkt odniesienia dla D-027, zmierzony na `--release` pod `MemoryMax=512M CPUQuota=100%`:
binarka **723 KB**, szczytowy RSS **2,2 MB**, 35 pozycji w drzewie zależności.
**To nie jest jeszcze dowód lekkości kompozytora** — to program, który wypisuje tekst i kończy
pracę. Wartość liczby jest inna: od teraz każdy wzrost ma punkt odniesienia i widać, co go
spowodowało.

### M1 — Pierwszy piksel, w oknie XFCE
Kompozytor na backendzie `winit`, renderer GLES2 **i** Pixman za wspólną abstrakcją.
Górny pasek: zegar (`cosmic-text`), ikony `[PROGRAMY]` / lupa / `[SYSTEM]`. Dolny pasek — pusty.
Środek — jednolite tło.

**Model wyjść (D-026):** wyjścia trzymane w **kolekcji**, każde z własną rozdzielczością, skalą
i transformacją (obrót). W v1 kolekcja ma jeden element, ale nic poza warstwą rysowania nie może
tego zakładać — telefon w stacji dokującej ma jednocześnie ekran pionowy i monitor poziomy.

**Gotowe, gdy:** `cargo run --backend winit` pokazuje okno z paskami; ten sam stan
zrenderowany Pixmanem zapisuje się do PNG i jest pierwszym wzorcem golden; licznik klatek
w spoczynku = 0; test jednostkowy layoutu przechodzi dla **dwóch wyjść o różnej orientacji
i skali** (bez kompozytora — sam model, D-016).

#### Od czego zacząć — kolejność, w której nic nie blokuje niczego

Każdy krok kończy się czymś, co da się uruchomić. Jeśli krok trwa dłużej niż pół dnia bez
widocznego efektu, jest za duży i trzeba go podzielić.

1. **`gostui-core`: warstwy ekranu.** Nowy moduł `shell.rs` — podział wyjścia na trzy strefy
   (górny pasek / obszar aplikacji / dolny pasek) w jednostkach logicznych. Wysokości pasków
   z konfiguracji, minimum 48 px dla dotyku (D-020). **Bez grafiki, z testami** — to samo
   miejsce co `layout::tile`, ta sama zasada.
2. **Szkielet kompozytora na `winit`.** Do `gostui-compositor` wchodzi `smithay`. Cel kroku:
   otworzyć puste okno w XFCE i zamknąć je czysto. Nic nie rysujemy. Jeśli to nie stoi,
   nie ma sensu iść dalej.
3. **Abstrakcja renderera.** Trait z dwiema implementacjami: GLES2 i Pixman. **Pixman najpierw** —
   jest prostszy, deterministyczny i to on daje golden PNG. GLES2 dokładamy, gdy Pixman rysuje.
4. **Trzy strefy w kolorze.** Trzy prostokąty z modelu z kroku 1. Pierwszy piksel. Tu wypada
   pierwszy wzorzec golden.
5. **Zegar w górnym pasku.** `cosmic-text` (D-005), aktualizacja **co minutę, nie co klatkę** —
   pierwszy prawdziwy test zasady „zero renderowania w spoczynku".
6. **Licznik klatek `GOSTUI_STATS=1`.** Bez niego kryterium „0 klatek na 10 s" jest opinią,
   nie pomiarem. ✅ **zrobione 2026-08-01** — razem z `--idle-test`, patrz §3.5.

**Zrobione 2026-07-30 wieczorem — kroki 1 i 4, poza kolejnością.** Powstał moduł
`gostui_core::shell` (trzy strefy + rozmieszczenie elementów górnego paska) oraz crate
`gostui-render` (rasteryzer software'owy + PNG). `cargo run -p gostui-compositor -- --png ui.png`
rysuje interfejs dla monitora i telefonu z tego samego stanu. 86 testów.

Pierwsze rysowanie od razu wyłapało błąd, którego żaden test dotąd nie widział: na pasku
o szerokości 360 jednostek zegar nachodził na oba sąsiednie elementy i pasek renderował się
jako jeden blok. Stąd `top_bar_layout` z jawną kolejnością poświęcania elementów i testem
sprawdzającym brak nachodzenia dla ośmiu szerokości. **Wniosek na przyszłość: rysuj wcześnie,
nawet bez okna — błędy layoutu są niewidoczne w liczbach, a oczywiste w obrazie.**

**Zrobione 2026-07-31 — krok 2.** `gostui-compositor` ma backend `winit` na smithayu 0.7.0
(D-028). `cargo run -p gostui-compositor -- --backend winit` otwiera okno w sesji XFCE,
wypełnia je kolorem pulpitu i zamyka się czysto. Nie ma jeszcze gniazda wayland ani klientów —
to M2. 88 testów.

**Zrobione 2026-07-31 — krok 3.** Abstrakcja renderera: `gostui_render::display_list` zamienia
stan shella na listę prostokątów w jednostkach logicznych, a `ShellRenderer` rasteryzuje ją
dwiema drogami — `draw_solid` w GLES2 albo własny rasteryzer na CPU, wgrywany jako jedna tekstura
(D-028, wariant 2). Wybór w czasie działania: `--renderer gles2|pixman`. 93 testy.

```bash
cargo run -p gostui-compositor -- --backend winit            # okno, do zamknięcia myszą
cargo run -p gostui-compositor -- --backend winit --renderer pixman   # ta sama treść z CPU
cargo run -p gostui-compositor -- --backend winit --frames 1 # test dymny: jedna klatka i wyjście
RUST_LOG=gostui=debug,smithay=info cargo run -p gostui-compositor -- --backend winit
```

**Weryfikacja, która wyszła lepiej, niż zakładałem:** zrzuty okna z obu ścieżek (`import -window
"GOST OS"`, 1360×850) są **identyczne co do piksela** — `compare -metric AE` daje 0. To nie jest
kosmetyka, tylko dowód, że jedna lista wyświetlania rzeczywiście wystarcza obu rendererom;
gdyby GLES2 i CPU liczyły geometrię osobno, różnica pojawiłaby się już na obwódce aktywnej karty.
Zrzut jest też pierwszym realnym obrazem shella z okna, a nie z pliku PNG.

**RSS `--release`, jedna klatka, okno zagnieżdżone:** GLES2 90,8 MB, CPU 95,3 MB.
*(Po kroku 5, z `cosmic-text`: GLES2 **94,4 MB**, CPU **99,0 MB**, `--png` bez GL **15,1 MB**,
binarka **7,7 MB**. Tekst kosztuje ~3,6 MB RSS i ~2,9 MB binarki — patrz D-005.)* Różnica to
canvas 1360×850×4 B plus tekstura. Uwaga: **to nie jest pomiar progu 80 MB dla Pixmana z D-029** —
w oknie zagnieżdżonym ścieżka CPU i tak trzyma kontekst EGL. Ten próg da się zmierzyć dopiero
na DRM (M4) albo na binarce bez GL.

`--frames n` istnieje po to, żeby „okno się otwiera i zamyka czysto" było asercją CI, a nie
czymś, na co ktoś raz spojrzał. Bez niego jedynym sposobem sprawdzenia jest kliknięcie krzyżyka.

**Sensowne jest wyłącznie `--frames 1`** — i wynika to wprost z punktu 1 niżej. W spoczynku
rysuje się dokładnie jedna klatka, więc każde `n > 1` czeka na klatki, które nie przyjdą, aż
do zamknięcia okna ręcznie. Sprawdzone 2026-08-01: `--frames 1` kończy się kodem 0 w ułamku
sekundy, `--frames 5` nie skończyło się przez 20 s. To nie jest usterka do naprawienia — to
zero renderowania w spoczynku widziane od strony wiersza poleceń. **CI tego nie uruchamia**
(`.github/workflows/ci.yml` buduje z cechą `winit`, nie odpala binarki); żeby uruchamiał,
potrzebny jest `xvfb-run` i twarde `--frames 1`.

**Trzy rzeczy, które ten krok ustalił — szczegóły w D-028:**
1. **`WinitEventLoop` jest źródłem `calloop`.** Pętla śpi w `poll`; przy 6 s stania z otwartym
   oknem i budżetem 100 klatek narysowała się **jedna** (ta początkowa). „Zero renderowania
   w spoczynku" jest zgodne z biblioteką, nie wbrew niej — o to było główne ryzyko.
2. **Backend `winit` jest GLES-owy z definicji** (`R: From<GlesRenderer> + Bind<EGLSurface>`),
   więc „Pixman najpierw" z kroku 3 nie da się zrobić przez okno zagnieżdżone. Do rozstrzygnięcia
   przed krokiem 3: `backend_x11` (prawdziwy Pixman, ale zależności `gbm`/`drm` od zaraz)
   albo wgrywanie obrazu z CPU jako tekstury (tanie, ale prawdziwy Pixman dopiero na DRM w M4).
3. **RSS 90,8 MB** na `--release`, przy jednej klatce z samym tłem. Ta sama binarka bez GL:
   3 MB (`--help`), 11,5 MB (`--png`). Około 79 MB bierze Mesa/radeonsi z EGL-em — **budżet
   80 MB z D-017/D-027 zjada sterownik, zanim dołoży się cokolwiek własnego.** Zanim budżet
   stanie się testem psującym build, trzeba rozstrzygnąć, czego dotyczy.

**Zrobione 2026-08-01 — krok 5.** Zegar w górnym pasku, tekst przez `cosmic-text` (D-005).
Lista wyświetlania ma teraz dwa prymitywy (`Fill`, `Text`); tekst jest rasteryzowany **raz**,
w `gostui-render`, a obie ścieżki tylko umieszczają gotowy obraz — bo dwie niezależne
implementacje shapingu nigdy nie trafią w te same piksele. 147 testów.

Trzy rzeczy, które ten krok ustalił:
1. **Zegar nie odpytuje.** `Wall::until_next_minute` w `gostui-core` mówi, ile spać; timer calloop
   budzi kompozytor dokładnie wtedy, gdy wyświetlana minuta staje się nieprawdziwa, a strażnik
   porzuca przerysowanie, jeśli napis się nie zmienił. To wzorzec dla kafli żywych (D-033).
2. **Niezmienniczość „identyczne co do piksela" trzeba było doprecyzować.** Prostokąty nadal są
   identyczne; glify zegara różnią się na **101 pikselach o 1/255**, w prostokącie 34×10, bo CPU
   blenduje w liczbach całkowitych, a GPU we `float`ach. Złote obrazy rysują więc powłokę
   **bez zegara**, a tekst ma własne testy layoutu i cache'u. Szczegóły i granice w D-005.
3. **`opaque_regions` to nie druga lista damage.** Skopiowanie wywołania `render_texture_at`
   ze ścieżki CPU deklarowało cały prostokąt glifu jako nieprzezroczysty, smithay wyłączał
   blendowanie i zegar rysował się na czarnym kaflu. Dla tekstu ten argument musi być pusty.
   **Błąd niewykrywalny testem jednostkowym — znalazło go porównanie dwóch zrzutów okna.**

**Uwaga na kolejność z kroku 3.** Naturalny odruch to zacząć od GLES2, bo „docelowy". Byłby to
błąd: renderer software'owy jest deterministyczny, więc od razu daje testy porównujące obraz,
a przy GPU pierwsze błędy są nie do odróżnienia od błędów sterownika.

**Zrobione 2026-08-01 — krok 6, ostatni w M1.** Licznik klatek `GOSTUI_STATS=1` (linia na klatkę
z **powodem** + raport przy zamknięciu) oraz `--idle-test <sekundy>`, czyli kryterium „zero
renderowania w spoczynku" jako kod wyjścia. Pełny opis, odstępstwo od pierwotnego „co sekundę"
i zmierzone liczby: **§3.5**. 156 testów.

Trzy rzeczy, które ten krok ustalił:
1. **Instrumentacja nie może się budzić.** Zapisany wcześniej „wypis co sekundę" wymagałby timera
   raz na sekundę — czyli dokładnie tego, czego zabrania krok 5 i D-033, i to w kodzie, który ma
   tego pilnować. Wypis per klatka daje mocniejszy sygnał (zero klatek = zero linii) przy zerowym
   koszcie w spoczynku.
2. **Pierwsza wersja kryterium dawała fałszywy alarm** — liczyła klatki od startu procesu, a XFCE
   przy mapowaniu okna przysyła `Resized` + `Redraw` w ciągu ~10 ms. Mierzone jest okno
   **po ustabilizowaniu**, a klatki zegara liczone osobno od klatek bez powodu. Dopiero takie
   kryterium mówi coś o naszej polityce rysowania, a nie o zarządzaniu oknami X11.
3. **Powód klatki jest wart więcej niż licznik.** „47 klatek" to liczba do interpretacji;
   „47 klatek, wszystkie `redraw`" wskazuje palcem na źródło. Zapisywanie powodu kosztowało
   jedno pole `enum` i będzie się zwracać przy każdym późniejszym backendzie.

**Czego świadomie nie zrobiono:** upadła klatka (błąd renderu) nie jest liczona — statystyki
opisują to, co trafiło na ekran. Liczenie porażek jako renderów zawyżałoby dokładnie tę liczbę,
którą czyta kryterium.

### M2 — Prawdziwy kompozytor ⚠️ punkt weryfikacji ryzyka
`xdg-shell`, `wl_seat` (klawiatura + mysz z `xkbcommon`), `wl_shm` + `linux-dmabuf`,
`wl_output`, `xdg-decoration`, **`wl_data_device` + primary-selection (schowek!)**,
`relative-pointer-v1` + `pointer-constraints-v1` (rdzeń wejścia — D-022).

**Model okien: kafelkowanie (D-025).** Okna dzielą obszar aplikacji automatycznie, nie nakładają się
i nie są przesuwane. Limit jednocześnie kafelkowanych okien (2 na wąskim ekranie, 2–3 na szerokim);
pozostałe czekają na dolnym pasku i wchodzą na miejsce wybranego kafelka. Podział wzdłuż dłuższej
osi ekranu. Suwak podziału przeciągany, proporcja zapamiętywana.
**Dialogi, okna wyboru pliku i popupy pozostają pływające** — kafelkowanie ich to najczęstszy
sposób, w jaki kafelkujące kompozytory stają się nieużywalne. Respektowanie `set_min_size` klienta.

**Gotowe, gdy:** `foot` działa jako pełnoprawny terminal; `gtk4-demo` i aplikacja Qt6 działają;
**kopiuj-wklej między dwoma klientami działa w obie strony**; dwa okna kafelkują się poprawnie,
a **okno wyboru pliku w GTK i Qt otwiera się jako pływające, nie jako trzeci kafelek**;
aplikacja z dużym `set_min_size` nie zostaje ściśnięta poniżej swojego minimum;
**odłączenie wyjścia, na którym stoją okna, nie wywala kompozytora — okna wracają na wyjście
pozostałe** (D-026; przy backendzie `winit` testowane na modelu, na tty przez odpięcie HDMI);
zabicie klienta nie rusza kompozytora; klient-fuzzer wysyłający błędne żądania zabija tylko siebie.

To najważniejszy etap w projekcie. Jeśli tu jest dobrze, reszta jest przewidywalna.

**Kolejność kroków** — każdy z osobnym kryterium, żeby ryzyko rozładowywało się po kawałku:

1. **Model okien w `gostui-core`** ✅ **zrobione 2026-08-01.** `WindowModel`: kafelki, kolejka
   na dolnym pasku, wymiana kafelka z fokusem, promocja przy zamknięciu, przeniesienie okien
   ze znikającego wyjścia. 21 testów, bez kompozytora — trzy pułapki D-025 są tu testami,
   nie komentarzami. Kryterium: `cargo test` opisuje zachowanie kafelkowania w całości.
2. **Gniazdo i `xdg-shell`** ✅ **zrobione 2026-08-01.** `wayland-gostui`, globale
   `wl_compositor`, `wl_shm`, `xdg_wm_base`, `wl_output`, `wl_seat`, `wl_data_device_manager`
   + primary-selection. `foot` startuje, dostaje `configure` z rozmiarem kafelka i pojawia się
   na dolnym pasku; wyjście klienta nie rusza kompozytora. Klatki tylko z powodu — z podłączonym
   `foot` 3 klatki na 12 s, 0 bez powodu. **Zawartość okien jeszcze się nie rysuje** — krok 3.
3. **Zawartość okien na ekranie** ✅ **zrobione 2026-08-01.** Okno to nowy wariant listy
   wyświetlania (`Primitive::Surface`), rozwiązywany osobno przez każdą ścieżkę: GLES importuje
   teksturę przez smithaya, CPU kopiuje bufor `wl_shm` do tego samego płótna, które pisze złote
   obrazy. `foot` wygląda tak samo na obu. Kolejność Z jest własnością jednej listy — pulpit,
   okna, paski — i jest testem. `frame` callbacki dostają **tylko okna widoczne**, więc okno
   czekające na dolnym pasku nigdy nie jest proszone o rysowanie. Zmierzone: 2 klatki na 14 s
   z otwartym terminalem, 0 bez powodu. **Zostaje:** `linux-dmabuf` (ścieżka CPU takiego okna
   nie odczyta — pomija je świadomie) i damage tylko uszkodzonych regionów.
4. **Wejście** ✅ **zrobione 2026-08-02.** `wl_seat` routowany naprawdę: klawiatura przez
   `xkbcommon`, skróty powłoki przechwytywane przed klientem (D-041), wskaźnik z ruchem względnym,
   `wl_touch` osobną ścieżką. Trafienie w strefę i tablica skrótów są w `gostui-core::input`
   i mają testy bez kompozytora. Globale `relative-pointer-v1` i `pointer-constraints-v1`
   są ogłaszane (potwierdzone `wayland-info`); blokada wskaźnika jest aktywowana, zamknięcie
   w regionie **nie** — patrz D-041. Zmierzone z dwoma terminalami: pisanie trafia do okna
   z fokusem, klik w drugi kafelek i klik w chip przenoszą fokus, `Super+Q` zamyka okno
   (`shell shortcut action=CloseWindow` → `toplevel destroyed`).
   **Trzy rzeczy, których ten krok nie załatwia i trzeba o nich wiedzieć:** `Super+Tab`
   w trybie zagnieżdżonym przechwytuje `xfwm4` (`switch_window_key`), więc do nas nie dociera —
   testowalne dopiero na tty albo po zwolnieniu skrótu w XFCE; kursor nadal rysuje sesja
   gospodarza, nie my; dotyku nie da się na tej stacji uruchomić, więc jego ścieżka jest
   napisana i nieprzetestowana na sprzęcie.
5. **Dekoracje i okna nietypowe** ⚠️ **zrobione 2026-08-02, jedno kryterium niesprawdzone.**
   `xdg-decoration` ogłaszane, każdy klient dostaje `ServerSide`; dekoracją jest **sama ramka
   fokusu** (D-043). Popupy pozycjonowane przez `xdg_positioner` — rozwiązywanie ograniczeń jest
   smithaya (flip → slide → resize), model przechowuje wynik. Toplevel z rodzicem to dialog:
   pływa, nie bierze kafelka. Pełny ekran zakrywa oba paski, wyjściem jest `Super+F` (D-042).

   **Ten krok zaczął się od usterki, która psuła wszystko pozycyjne od kroku 4.** W smithayu para
   „fokus" niesie pozycję powierzchni w układzie globalnym i biblioteka sama ją odejmuje;
   przekazywaliśmy już odjętą pozycję lokalną, więc klient dostawał `globalna − lokalna`, czyli
   swój własny róg, zamrożony. Menu się nie otwierały, przyciski nie reagowały, przeciąganie nie
   działało — a pisanie działało bez zarzutu, bo klawisze nie niosą współrzędnych. **Wniosek na
   przyszłość: „klawiatura działa, mysz nie" prawie nigdy nie znaczy „mysz nie dochodzi", tylko
   „dochodzi z fałszywą pozycją".**

   Zweryfikowane na ekranie: menu „Plik" w Mousepadzie (GTK) i menu podręczne w `weston-terminal`
   otwierają się, są rysowane i znikają; okna wyrównują się do kafelka co do piksela po odjęciu
   marginesu cienia z geometrii klienta.

   **Niesprawdzone było: okno wyboru pliku jako pływające** — domknięte w kroku 6, i okazało się
   usterką, nie brakiem pakietu.
6. **Domknięcie** ✅ **zrobione 2026-08-02.** `gtk4-demo`, Qt6, kopiuj-wklej, klient-fuzzer.
   **Krok znalazł trzy usterki, z których dwie były w kodzie uznanym za gotowy od kroków 2 i 5** —
   i to jest główny wynik tego kroku, ważniejszy od samych odhaczonych kryteriów.

   **Klient-fuzzer** (`crates/gostui-fuzz-client`, D-045): 17 scenariuszy, **kompozytor żyje po
   każdym, na obu ścieżkach renderera**, kod wyjścia 0. Siedem scenariuszy idzie surowymi bajtami,
   bo są niewyrażalne przez typowane API klienta; dziesięć używa biblioteki poprawnie, żeby dojść
   tam, gdzie kompozytor **wierzy liczbom** klienta. Uruchomienie: `WAYLAND_DISPLAY=wayland-gostui
   cargo run -p gostui-fuzz-client`, `--list` pokazuje scenariusze.

   **Znalazł panikę osiągalną z każdego klienta:** `set_window_geometry(0, 0, -1, -1)` →
   `Size::new` w smithayu → koniec kompozytora i wszystkich aplikacji użytkownika. Naprawione
   filtrem przed smithayem (D-045 wyjaśnia, dlaczego nie `catch_unwind`).

   **Kopiuj-wklej działał w jedną stronę i to wyglądało jak brak danych.** `set_data_device_focus`
   i `set_primary_focus` nie były wołane **nigdzie w repozytorium**, więc smithay nie miał komu
   wydać selekcji: `wl-copy` ustawiał schowek, `wl-paste` dostawał `wl_keyboard.enter` i czekał
   w nieskończoność na `wl_data_device.selection`. Pusty wklej wygląda dokładnie jak pusty
   schowek — dlatego to przeżyło od kroku 2. Test: `scripts/test-schowek.sh`, pięć rund
   (schowek w obie strony, primary w obie strony, przejęcie schowka przez trzeciego klienta),
   **5/5**.

   **Okno wyboru pliku było kafelkowane — pułapka 2 z D-025 wprost.** Nie z braku kodu: rola była
   ustalana w `new_toplevel`, a `xdg_toplevel.set_parent` przychodzi **po** `get_toplevel`, więc
   każdy dialog rodził się bezrodzicielski. Rodzic dołączył do rzeczy odczytywanych na commicie,
   obok tytułu i `min_size`. Zweryfikowane na ekranie: okno „Wybierz plik" pływa wyśrodkowane nad
   rodzicem, rodzic trzyma cały kafelek, na dolnym pasku **jeden** chip.

   **`gtk4-demo` i Qt6 rysują się na obu ścieżkach.** Największe przewidywane ryzyko —
   `linux-dmabuf`, którego kompozytor **nie ogłasza w ogóle** — nie zmaterializowało się: GTK4
   4.14 spada na `wl_shm` (widać po `libEGL warning: failed to get driver name`) i nie ma ani
   jednego błędu importu bufora. Qt6 nie rysuje własnego nagłówka, więc dostaje samą ramkę fokusu
   (D-043); GTK4 rysuje swój, zgodnie z tym, co D-043 już zapowiadał.

   **Czego ten krok nie załatwia:** `linux-dmabuf` nadal nie jest ogłaszany — klient, który go
   wymaga bezwzględnie, nie ma ścieżki. Nadal brakuje też damage tylko uszkodzonych regionów
   (D-027) i testu budżetu pamięci psującego build (D-038).

### M3 — Slider kart i Menu Start (serce produktu)
Slider: nawigacja `Super+←/→` + gołe strzałki przy fokusie slidera + klik, snap bez animacji,
skrawek sąsiedniej karty z regułą responsywną, tryb edycji (reorder), przypinanie (jedna karta,
rezerwuje przestrzeń), `[+] Nowa karta`, ikony funkcyjne (resize / tylko ikony / sortowanie),
stan per karta zapisywany atomowo. Menu Start z drzewa `~/gostui/menu_start/` + `inotify` +
`gostui-menu-sync` (bootstrap z `/usr/share/applications` po `Categories`, nowe pozycje do
`Nieskategoryzowane/`, nigdy nie rusza układu użytkownika). Wyszukiwanie: `Super`/`Super+S`,
dopasowanie po `Name`+`Comment`+`Keywords`, Enter uruchamia. Motyw ikon + SVG + cache.

**Gotowe, gdy:** zestaw golden PNG pokrywa każdy tryb karty; utworzenie folderu w menedżerze
plików XFCE w `menu_start/` pojawia się w menu bez restartu; uruchomienie aplikacji z menu działa
(w tym `Terminal=true`); restart kompozytora odtwarza układ kart 1:1; zmiana karty < 16 ms.

**Stan M3 (2026-08-02): zaczęte od brzegów, sedno zablokowane decyzjami.**

Zrobione:

1. **Ikona Menu Start** — cztery kwadraty w siatce 2×2, rysowane czterema wypełnieniami koloru
   paska wyciętymi z akcentowanego przycisku. Bez tekstury i bez glifu, więc obie ścieżki
   renderera dają ten sam obraz; geometria (`shell::menu_icon`) siedzi w core razem
   z `top_bar_layout`, bo to arytmetyka (D-016). Przycisk zwężony do kwadratu 48×48 — jest przez
   to dokładnie celem dotykowym (D-020) i zwalnia 84 jednostki paska.
2. **Złote obrazy** — `crates/gostui-render/tests/golden/`, cztery sceny porównywane co do
   piksela przez zwykłe `cargo test`. **To był niedotrzymany warunek D-010**, nie nowy pomysł:
   tamta decyzja wybrała „CPU do tekstury" zamiast drugiego backendu pod warunkiem, że ścieżka
   CPU zostanie pokryta testami obrazu, a §3.2 tego dokumentu opisuje je od początku.
   Że warunku nie dotrzymano, było widać od razu — ikona z punktu 1 zmieniła wygląd powłoki
   i **żaden test tego nie zauważył**. Obsługa: `GOSTUI_BLESS=1`, szczegóły w `docs/04`.

   Sceny **nie zawierają tekstu** (zegar to jedyny tekst powłoki, więc `clock: None` usuwa
   glify) i dzięki temu wychodzą identycznie tutaj i na maszynie CI — sprawdzone, nie założone.
   Koszt: `check` w CI wydłużył się z ~57 s do ~1 min 36 s.

**Czego nie zaczynać bez rozmowy:** dawniej był to sam slider, przez odstępstwo od `gostos.md` §B
zapisane w D-031. **Odstępstwo zniknęło, nie zostało wynegocjowane** (2026-08-03): karta jako
kolumna o stałej szerokości (D-046) daje skrawek sąsiedniej karty wprost ze specyfikacji — to
po prostu wygląd kolumny, która się nie zmieściła. D-031 ma status ZASTĄPIONA, a pytania
do człowieka w tym miejscu już nie ma. D-030 i D-033 nadal czytaj przed kodem; D-008 i D-009 mają
rekomendacje przyjmowane domyślnie i wystarczy jawnie zaznaczyć, że się je bierze (tak weszło
D-007 — `Super+←/→`, 2026-08-04). Tabela: `docs/04-zasady-pracy.md`.

Uwaga praktyczna: środkowa strefa jest już narysowana kolumnami, a złote obrazy utrwalają ten
układ — w tym scenę z przewiniętym paskiem. Kolejna zmiana wyglądu karty zaprotestuje w `cargo
test` i tak ma być; `GOSTUI_BLESS=1` dopiero po obejrzeniu obu plików.

### M4 — Goły metal
Backend `udev`/DRM/KMS + `libinput` + `seatd`, przełączanie VT, obsługa uśpienia/wybudzenia.
Plik sesji, uruchamianie z usługi systemd użytkownika z `Restart=on-failure`.

**Gotowe, gdy:** na tty3 kompozytor startuje na Vega 11 w 1920×1080, mysz i klawiatura działają,
`Ctrl+Alt+F1` wraca do XFCE i powrót na tty3 nie psuje obrazu; awaria kompozytora skutkuje
restartem z odtworzonym stanem kart.

### M5 — XWayland ⚠️ drugi punkt weryfikacji ryzyka
Serwer XWayland, mapowanie okien X11 na model okien, okna override-redirect (menu, tooltipy),
mostek zaznaczenia X11 ↔ Wayland.

**Gotowe, gdy:** `xterm` i `xeyes` działają; VICE (C64) startuje i przyjmuje wejście z klawiatury;
kopiuj-wklej działa **między** aplikacją X11 i Wayland. Po tym etapie cała Warstwa 2 jest
kwestią `apt install`, nie pisania kodu.

### M6 — Menedżer plików (pierwsza własna aplikacja)
Za VFS (lub `sshfs`, wg decyzji z przeglądu 5.1). Backend lokalny: breadcrumb, trzy widoki,
menu kontekstowe, skróty (Ctrl+C/V/X, Delete, F2, Ctrl+A), zaznaczanie wielokrotne i lasso,
pasek statusu, właściwości, kosz zgodny z freedesktop (z regułą dla innych systemów plików),
ukryte pliki, konflikty nazw, skojarzenia przez `mimeapps.list`, tryb dwupanelowy z DnD wewnętrznym,
wyszukiwanie rekurencyjne w wątku roboczym ze strumieniowaniem wyników i anulowaniem.
Sekcje ekranu głównego: dyski lokalne / wymienne / sieciowe (`udisks2` przez D-Bus).

**Dochodzi z D-025:** **przeciąganie plików między osobnymi oknami** (pełne DnD przez
`wl_data_device`). Kafelkowanie wybrano m.in. dla tego zastosowania, więc bez niego główny zysk
z modelu okien nie istnieje. Tryb dwupanelowy w samym menedżerze zostaje jako ścieżka tania
(jeden proces, DnD wewnętrzny) i jako zabezpieczenie.

**Gotowe, gdy:** operacje na 10 000 plików nie blokują UI; usunięcie do kosza jest widoczne
w koszu XFCE i odwrotnie; wpięcie pendrive'a pokazuje go bez odświeżania; SFTP (jeśli w zakresie)
zerwane w trakcie kopiowania daje komunikat, nie zawieszenie; **przeciągnięcie pliku z jednego
okna menedżera do drugiego, sąsiedniego kafelka, kopiuje plik**.

### M7 — Panel systemowy i kontrakty D-Bus
Panel `[SYSTEM]`: głośność (PipeWire/WirePlumber), sieć (NetworkManager), bateria (UPower — na tej
maszynie nieobecna, więc sekcja musi się poprawnie ukrywać), jasność (**ukryta**, brak backlight),
uśpij/restart/wyłącz (`logind`). Plus **demon powiadomień** i **tray (StatusNotifierItem)**
z przeglądu 3.3/3.4. Portale: instalacja i poprawne `XDG_CURRENT_DESKTOP`.

**Gotowe, gdy:** `notify-send "test"` pokazuje dymek; aplikacja z trayem (np. Nextcloud/Steam)
chowa się i przywraca; zmiana głośności widoczna w `wpctl`; brak baterii i brak backlight nie
generują pustych kontrolek; udostępnianie ekranu w Firefoksie działa przez portal.

### M8 — Menedżer usług i Panel sterowania
Tabela usług z systemd przez `zbus` (start/stop/restart), siatka kategorii Panelu sterowania jako
nakładki na NetworkManager / PipeWire / wyświetlacz / konta / data-godzina. **Agent polkit
uruchamiany przez sesję** (przegląd 3.6).

**Gotowe, gdy:** restart usługi z UI pokazuje okno autoryzacji polkit i faktycznie działa;
odmowa autoryzacji daje czytelny komunikat, nie „not authorized".

### M9 — Wdrożenie
Pakiet `.deb`, plik sesji, dokumentacja instalacji, nadzorca, zapis/odtwarzanie stanu, test
wytrzymałościowy 24 h.

**Gotowe, gdy:** czysta maszyna wirtualna z Debianem minimalnym + `apt install ./gostui.deb`
daje działającą sesję; 24 h z Firefoksem i terminalem bez wzrostu RSS i bez awarii;
progi wydajności z przeglądu 6.2 spełnione i zmierzone.

### M10 — Telefon i pozostałe porty
`text-input-v3` + `input-method-v2` + integracja **`squeekboard`** (nie pisać własnej klawiatury
ekranowej). Port na Raspberry Pi (cross-compilacja `aarch64-unknown-linux-gnu` z tej stacji —
budowanie na samym RPi3 jest zbyt wolne; D-002). Weryfikacja telefonowa na urządzeniu **SDM845
z pmaports `community` i wyjściem DisplayPort alt mode** — OnePlus 6/6T (`enchilada`/`fajita`)
albo Pocophone F1 (`beryllium`); D-026. Dopiero potem device tree dla `rav` (Moto G8).
Opcjonalnie: własny greeter, eksperyment z `wgpu`.

**Gotowe, gdy:** shell startuje na telefonie, obsługuje dotyk i obrót ekranu, klawiatura ekranowa
pozwala pisać, **podłączenie monitora przez USB-C daje drugie wyjście z niezależnym layoutem
kafelków, a jego odłączenie nie gubi okien**, a czas pracy na baterii nie odbiega rażąco
od stanu z wygaszonym ekranem.

**Uwaga:** port `rav` to **osobny projekt niż shell** — ale jego zakres istotnie zmalał (D-023):
aparat, modem i GPS są wyłączone, więc odpada to, co przy mainlinowaniu telefonu generuje
najwięcej pracy. Zostaje panel DSI, dotyk, GPU, Wi-Fi, **Bluetooth** (krytyczny), dźwięk,
bateria, USB, uśpienie. Szczegóły: `03-cel-telefon.md` §7.

### Wymagania telefonowe wpięte we wcześniejsze etapy
Cel telefonowy nie przestawia harmonogramu, ale dokłada wymagania do istniejących etapów:

| Etap | Co dochodzi |
|---|---|
| M1 | transformacja wyjścia (obrót) i skala jako pola modelu wyjścia |
| M2 | `wl_touch` jako osobna ścieżka wejścia; `relative-pointer-v1` i `pointer-constraints-v1` — rdzeń wejścia, nie „dla gier" |
| M3 | manipulacja bezpośrednia przy przesuwaniu kart (D-021); reguły responsywne; cele dotykowe ≥ 48 px; dotykowe odpowiedniki `Super+←/→` i `Super+D`; **tryb wskaźnika** (D-022) — kursor rysowany przez kompozytor, ruch względny, gesty myszy, sposób przełączania |
| M5 | **awans priorytetu** — XWayland jest warunkiem podstawowego zastosowania (D-024), nie bonusem na Warstwę 2 |
| M7 | `idle-notify`, `idle-inhibit`, wygaszanie i uśpienie ekranu |

Do testowania dotyku **nie trzeba telefonu** — wystarczy tani ekran dotykowy USB podłączony
do stacji albo oficjalny ekran do Raspberry Pi. **Tryb wskaźnika testuje się tam tak samo dobrze
jak na telefonie**, więc nie jest zablokowany do M10.

---

## 5. Nawyki pracy na tej maszynie

- **Domyślnie Tier 1.** Na tty (Tier 2) wchodzić tylko po zakończeniu etapu, nie w trakcie iteracji.
- **`RUST_BACKTRACE=1` zawsze**, logi przez `tracing` z filtrowaniem po module
  (`RUST_LOG=gostui_compositor::shell=debug`).
- **`cargo build` w trybie debug do pracy**, ale wydajność mierzyć **tylko na `--release`** —
  różnica w rendererze programowym jest rzędu wielkości i debug-build da fałszywy alarm.
- **Zależności systemowe zapisywać na bieżąco** w `docs/zaleznosci.md` w miarę dodawania.
  Odtworzenie tej listy przy pakowaniu (M9) po fakcie jest niepotrzebnie żmudne.
- `sccache` lub `mold` jako linker — czas linkowania kompozytora ze smithayem szybko staje się
  najdłuższą częścią cyklu.
- Repozytorium **nie jest jeszcze zainicjowane jako git** — zrobić to przed M0.

---

## 6. Rejestr ryzyk

| Ryzyko | Waga | Kiedy się objawi | Reakcja |
|---|---|---|---|
| `wgpu` wymaga własnego interopu dmabuf | wysoka | M2 | Renderer GLES2 smithaya (przegląd 2.1) |
| RPi3 nie obsługuje `wgpu` (GLES 2.0) | wysoka dla portu | M10 | RPi4/5 albo renderer Pixman |
| XWayland trudniejsze niż założono | wysoka | M5 | Rozładowane wcześnie; Warstwa 2 osunięta, rdzeń nietknięty |
| Brak schowka wykryty późno | wysoka | M2 | Wpisane wprost w kryterium M2 |
| Rozjazd zakresu w menedżerze plików | średnia | M6 | Podział na „fundament" i „dodatki" ze specyfikacji jest dobry — trzymać się go |
| Greeter zjada czas rdzenia | średnia | dowolnie | Wypchnięty do M10, autologin w developmencie |
| Awaria kompozytora niszczy konfigurację | średnia | dowolnie | Zapis atomowy od M0 |
| Skalowanie dorabiane po fakcie | średnia | M10 | Jednostki logiczne od M1 |
| Blokada wejścia na tty | niska, ale bolesna | M4 | `timeout`, SysRq, SSH (sekcja 2.2 Tier 2) |
