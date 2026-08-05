# Rejestr decyzji projektowych (ADR)

Jeden wpis = jedna decyzja, która wpływa na architekturę. Wpisy nie są usuwane — zmiana decyzji
to nowy wpis z odwołaniem do poprzedniego. Status: `OTWARTA` / `PRZYJĘTA` / `ODRZUCONA` / `ZASTĄPIONA`.

---

## D-001 — Renderer kompozytora
**Status:** ✅ **PRZYJĘTA** (2026-07-30) — GLES2 + Pixman ze smithaya
**Decyzja:** Oba renderery smithaya za **wspólną abstrakcją** (`Renderer` jako trait wyboru
w czasie działania). `wgpu` odrzucone jako fundament — wymagałoby własnej warstwy interopu
dmabuf → tekstura, pisanej **zanim** pojawi się pierwsze okno obcej aplikacji.
**Konsekwencje:**
- Import buforów klienta (`wl_shm`, `linux-dmabuf`), damage tracking i kompozycja wyjścia
  są gotowe — nie piszemy ich.
- **Pixman (CPU) obecny od M1**, nie dopisany później: umożliwia deterministyczne testy
  golden PNG w CI bez GPU (`01-strategia-dev-test.md` §3.2) i stanowi fallback dla słabego sprzętu.
- Abstrakcja renderera musi powstać w M1, razem z pierwszym pikselem — nie da się jej dorobić potem.
- Świadomie przyjęty koszt: związanie z API smithaya, trudniejsze nowoczesne efekty graficzne
  (których specyfikacja i tak nie chce — „bez efektów 3D/perspektywy").
- `wgpu` pozostaje możliwym eksperymentem po M5, za tą samą abstrakcją.
**Odniesienie:** `00-przeglad-specyfikacji.md` §2.1

## D-002 — Raspberry Pi 3 jako cel
**Status:** ✅ **ROZWIĄZANA przez D-001** (2026-07-30) — RPi3 pozostaje możliwy
**Kontekst:** VideoCore IV daje OpenGL ES 2.0; backend GL w `wgpu` wymaga GLES 3.0, a Vulkan
na RPi3 nie istnieje — `wgpu` nie uruchomiłby się tam wcale.
**Rozwiązanie:** Po odrzuceniu `wgpu` (D-001) problem znika: renderer GLES2 smithaya działa
na GLES 2.0, a renderer Pixman nie potrzebuje GPU w ogóle. RPi3 zostaje w planach jako cel
portu (M10), z zastrzeżeniem, że wydajność trzeba będzie zmierzyć, nie założyć.
**Potwierdzenie użytkownika (2026-07-30):** „robimy pod RPi3, ale pierw PC — to jest to samo".
Zgadza się co do kodu: ta sama architektura kompozytora, ten sam renderer, ten sam zakres
protokołów. Różnice są wyłącznie warsztatowe, nie projektowe:
- **kompilacja** — budowanie na samym RPi3 jest bolesne; docelowo cross-compilacja
  (`aarch64-unknown-linux-gnu`) z tej stacji, obraz gotowy do skopiowania;
- **wydajność** — RPi3 jest pierwszym sprzętem, na którym progi z D-017 mogą nie wyjść;
  to jego rola w harmonogramie (M10), a nie efekt uboczny.
**Odniesienie:** §2.2

## D-003 — Relacja kart do okien aplikacji
**Status:** ✅ **PRZYJĘTA** (2026-07-30) — **Model A**
**Decyzja:** Slider jest warstwą pulpitu. Karty i okna są całkowicie niezależne: zmiana karty
nie rusza żadnego okna, okna zawsze zakrywają slider. Powrót do slidera przez akcję
**„Pokaż pulpit" (`Super+D`)**, chowającą wszystkie okna.
**Konsekwencje:**
- `Tab` w `gostui-core` **nie zawiera żadnych referencji do okien** — model kart i model okien
  są rozłączne i testowalne osobno.
- Dolny pasek pokazuje **zawsze wszystkie** otwarte okna, niezależnie od aktywnej karty.
- Trzeba zaimplementować `Super+D` (chowanie/przywracanie wszystkich okien) — bez tego przy jednym
  otwartym oknie slider staje się nieosiągalny.
- Odrzucono Model B (karty jako wirtualne pulpity) — mimo że jest bliższy sformułowaniom
  specyfikacji („karta Projekt Python: terminal + kod + dokumentacja"), kosztuje przypisywanie
  i przenoszenie okien między kartami oraz grozi „gubieniem" okien na nieaktywnych kartach.
**Powrót do B w przyszłości:** możliwy, ale będzie wymagał zmiany modelu danych — świadomie
nie przygotowujemy pod niego struktury.
**Odniesienie:** §4.1

## D-004 — Dostęp do plików: VFS czy `sshfs`
**Status:** OTWARTA — **blokuje M6**
**Opcje:** (a) własny trait VFS + backend SFTP w procesie, (b) montowanie przez `sshfs`, zwykłe ścieżki.
**Kompromis:** (b) jest szybsze do v1, ale traci kontrolę nad błędami połączenia, których specyfikacja
wprost wymaga (komunikat, nie zawieszenie). (a) kosztuje kilka godzin na starcie i chroni przed
przepisywaniem menedżera.
**Odniesienie:** §5.1

## D-005 — Stack tekstowy
**Status:** ✅ **PRZYJĘTA** (2026-08-01) — wdrożona w `gostui_render::text`
**Decyzja:** `cosmic-text` zamiast `fontdue`/`ab_glyph`. Rasteryzer sam nie daje odnajdywania fontów,
shapingu ani łamania linii.

**Jak tekst wchodzi do architektury dwóch rendererów.** Prostokąt obie ścieżki potrafią policzyć
niezależnie i się zgodzić. Glify — nie: shaping i hinting są zbyt subtelne, żeby dwie implementacje
trafiły w te same piksele. Dlatego tekst jest **rasteryzowany raz**, w `gostui-render`, do RGBA8
z już nałożonym kolorem, a obie ścieżki tylko umieszczają wynik: CPU blenduje go w canvas, GLES2
wgrywa jako teksturę. Lista wyświetlania ma teraz dwa warianty (`Primitive::Fill` i `::Text`),
rozwiązywane do `Painted::Fill` / `::Image` **przed** otwarciem klatki.

**Granica warstw:** `gostui-core` decyduje, **w którym prostokącie** stoi napis (to arytmetyka
z testowalną odpowiedzią, D-016). Gdzie glify siedzą **wewnątrz** tego prostokąta, wie dopiero
warstwa tekstu, bo do tego trzeba zmierzyć szerokość, a do tego trzeba fontu. Nic w core nie wie,
że font istnieje.

**Niezmienniczość „identyczne co do piksela" wymaga teraz doprecyzowania.** Pomiar 2026-08-01,
oba zrzuty z tej samej minuty, okno 1360×842:
- **prostokąty: nadal identyczne co do piksela** — różnice mieszczą się w prostokącie 34×10
  na samych cyfrach zegara i nigdzie indziej;
- **krawędzie antyaliasingu: 101 pikseli różniących się o dokładnie 1/255.** Nieusuwalne: ścieżka
  CPU blenduje w 8-bitowych liczbach całkowitych, GPU we `float`ach, a wynik zapisuje do bufora
  8-bitowego. Zaokrąglenie zamiast obcinania po obu stronach zmniejszyło błąd do jednego bitu
  najmniej znaczącego; wyzerować się go nie da bez naginania jednej ze ścieżek do arytmetyki
  drugiej.
- **Wniosek dla testów:** złote obrazy porównujące piksele rysują powłokę **bez zegara**, a tekst
  jest testowany przez swój layout i cache, nie przez piksele. Pikselowy test tekstu wymagałby
  fontu w repozytorium (bo rodzinę rozwiązuje fontconfig, więc wynik zależy od maszyny) — decyzja
  z kosztem rozmiaru, nie podjęta.

**Błąd wart zapamiętania, bo wygląda na szum, a nie jest.** `render_texture_at` przyjmuje osobno
`damage` i `opaque_regions`. Skopiowanie wywołania ze ścieżki CPU — gdzie tekstura naprawdę jest
nieprzezroczysta — deklarowało cały prostokąt glifu jako nieprzezroczysty, więc smithay wyłączał
blendowanie i **zegar rysował się na czarnym kaflu**. Dla tekstu `opaque_regions` musi być puste.

**Koszt pamięci (2026-08-01, `--release`):** binarka 4,8 → **7,7 MB**; RSS ścieżki czysto CPU
(`--png`, bez GL) 11,5 → **15,1 MB**; okno GLES2 90,8 → **94,4 MB**; okno Pixman 95,5 → **99,0 MB**.
Tekst kosztuje więc ~3,6 MB RSS — mieści się w prognozie 1–4 MB z D-032 i zostawia duży zapas
do progów z D-029.
**Odniesienie:** §2.3, D-001, D-016, D-028, D-029, D-032

## D-006 — Greeter poza warstwą Core
**Status:** OTWARTA (rekomendacja: przyjąć)
**Decyzja:** Greeter to drugi kompozytor — najwyższy koszt, najniższa wartość. Development na
autologinie; jeśli potrzebny wybór sesji, `greetd` + `tuigreet`. Własny frontend w M10.
**Odniesienie:** §2.5

## D-007 — Nawigacja slidera z klawiatury
**Status:** ✅ **PRZYJĘTA** (2026-08-04) — rekomendacja przyjęta i zrealizowana w kodzie
**Decyzja:** `Super+←/→` globalnie; gołe strzałki tylko gdy fokus ma slider. Gołe strzałki globalnie
zepsułyby każde pole tekstowe w każdej aplikacji.

**Co weszło (2026-08-04):** `Action::ActivateNextCard` / `ActivatePreviousCard` w `gostui-core`
i dwa skróty w domyślnej mapie. Skrót działa **niezależnie od okien** — karty i okna są rozłączne
(D-003), więc przesunięcie paska nie zmienia fokusu okna i nie wysyła klientom niczego.

**Czego świadomie nie ma:** gołych strzałek. „Slider ma fokus" nie jest stanem, w którym powłoka
może dziś być, a skrót na stan nieosiągalny jest albo kodem martwym, albo — jeśli kiedyś trafi —
strzałką odebraną polu tekstowemu. Wejdą razem z fokusem slidera, nie wcześniej; pilnuje tego
asercja w `the_arrows_move_the_slider_only_when_super_is_held`.

**Konsekwencja dla zera renderowania (D-027):** pasek nie zawija się na końcach, więc przy skrajnej
karcie wciśnięcie nie zmienia nic **i nie rysuje klatki**. Bez tego przytrzymana strzałka jest
pętlą renderującą — najtańszy sposób na złamanie wymagania, które kosztowało cały M2.

**Kolizja z sesją-gospodarzem, nie usterka:** `xfwm4` trzyma `Super`+strzałki, więc w trybie
zagnieżdżonym skrót nie dociera do nas, dopóki nie puści ich `scripts/xfce-zwolnij-skroty.sh`.
Na gołym metalu (M4) problem nie istnieje.
**Odniesienie:** §4.2, D-003, D-027, D-046

## D-008 — Zawartość karty
**Status:** OTWARTA (rekomendacja: przyjąć i wpisać do specyfikacji)
**Decyzja:** Karta = siatka elementów (skrót do aplikacji / folderu / zamontowany dysk / plik).
**Nie** osadza widoków obcych aplikacji.
**Odniesienie:** §4.5

## D-009 — Karta przypięta
**Status:** OTWARTA
**Decyzja proponowana:** maksymalnie jedna; rezerwuje przestrzeń ekranu (okna jej nie zakrywają);
znika ze slidera na czas przypięcia.
**Odniesienie:** §4.4

## D-010 — Wiele monitorów poza zakresem v1
**Status:** ⚠️ **ZREWIDOWANA** (2026-07-30) — patrz D-026
**Decyzja:** v1 obsługuje jedno wyjście (stacja ma jeden HDMI 1920×1080), ale kod trzyma wyjścia
w kolekcji, nie w jednym polu.
**Rewizja:** warunek, który sam zapisałem („gdyby w grę wszedł monitor zewnętrzny, wiele wyjść
wraca do zakresu"), został spełniony — użytkownik szuka telefonu z wyjściem obrazu. Wiele wyjść
nadal nie wchodzi do v1 jako funkcja, ale **przestaje być rzeczą do dopisania później**: model
wyjść musi od M1 dopuszczać ich wiele, z niezależną skalą i transformacją każdego. Patrz D-026.
**Odniesienie:** §4.9

## D-011 — Skalowanie od początku, choćby zawsze 1.0
**Status:** OTWARTA (rekomendacja: przyjąć)
**Decyzja:** Layout liczony w jednostkach logicznych, skala per wyjście mnożona przy rasteryzacji.
Koszt teraz bliski zeru, koszt później to przepisanie layoutu. Cel „telefon" tego wymaga.
**Odniesienie:** §4.9

## D-012 — Zakres protokołów Wayland w v1
**Status:** OTWARTA
**Kontekst:** Specyfikacja nie zawiera listy protokołów, a ona decyduje, czy dana aplikacja działa.
Tabela minimum: §3.7. Do rozstrzygnięcia osobno: `wlr-layer-shell` (potrzebny tylko dla zewnętrznych
paneli — paski piszemy sami, ale wsparcie daje awaryjną drogę do gotowych narzędzi).
**Rozstrzygnięte przyrostowo (2026-08-01), patrz D-036:** `linux-dmabuf-v1`, `wp_viewporter`,
`presentation-time`, `idle-inhibit-v1` **wchodzą do zakresu** — wynikają ze scenariusza odtwarzania
wideo na słabym sprzęcie, a pierwszy z nich jest warunkiem koniecznym, nie optymalizacją.
**Do rozstrzygnięcia przy D-035:** `wlr-screencopy-v1` oraz wirtualna klawiatura i wskaźnik
(zdalny pulpit). Nie blokują niczego przed M7.
**Odniesienie:** §3.7, D-035, D-036

## D-013 — Elementy dopisane do zakresu, by obce aplikacje działały
**Status:** OTWARTA
**Do zaakceptowania jako część zakresu:** XWayland (M5), schowek `wl_data_device` (M2),
demon powiadomień (M7), tray StatusNotifierItem (M7), portale przez `xdg-desktop-portal-gtk` (M7),
agent polkit w sesji (M8).
**Odniesienie:** §3

## D-014 — Nazwa projektu i licencja
**Status:** ✅ **PRZYJĘTA** (2026-07-30) — dwupoziomowa
**Kontekst:** Odnalezione repozytorium `ventura85/GOST-OS` zawiera gotową identyfikację wizualną:
logo z podpisem **„GOST OS — Gostynin's operating system"**. Nazwa ma znaczenie i markę, więc
wcześniejsza rekomendacja („GostUI jest uczciwsze, bo to nie system operacyjny") została wycofana.
**Decyzja:**
- **GOST OS** — marka, nazwa repozytorium, docelowa dystrybucja.
- **`gostui`** — nazwa techniczna shella: prefiks crate'ów (`gostui-core`, `gostui-compositor`),
  binarka `gostui`, konfiguracja `~/.config/gostui/`, menu `~/gostui/menu_start/`,
  sesja `/usr/share/wayland-sessions/gostui.desktop`.
Shell jest komponentem systemu — obie nazwy współistnieją bez konfliktu.
**Licencja:** ✅ **GPL-3.0** (2026-07-30), plik `LICENSE` w repozytorium. Zgodna z ekosystemem
Wayland/DE. Konsekwencja: wszystkie zależności muszą być licencyjnie kompatybilne z GPL-3.0 —
kontrolowane automatycznie przez `cargo deny check` w CI (Apache-2.0 i MIT, dominujące w ekosystemie
Rust, są kompatybilne).

**Zapis identyfikatora — pułapka, która już raz wywaliła CI:** w `Cargo.toml` i w `deny.toml`
obowiązuje **`GPL-3.0-only`**, nigdy samo `GPL-3.0`. Krótsza forma jest przestarzałym
identyfikatorem SPDX, a `cargo-deny` porównuje dokładnie — lista dozwolonych z `GPL-3.0`
odrzuciła wszystkie cztery **nasze własne** crate'y jako nielicencjonowane.
Wybór `-only` (a nie `-or-later`) jest świadomy: nie zobowiązuje nas do zgodności z licencjami,
których jeszcze nie widzieliśmy.

**Prywatność repozytorium a GPL:** obowiązek udostępnienia źródeł powstaje dopiero przy
rozpowszechnianiu binariów. Repozytorium prywatne (patrz D-019) nie jest z GPL-3.0 w konflikcie.

## D-020 — Telefon jako platforma docelowa, PC i RPi jako droga do niej
**Status:** ✅ **PRZYJĘTA** (2026-07-30)
**Decyzja:** Celem docelowym jest telefon (Motorola Moto G8 i dowolny inny). Rozwój prowadzony
na PC, następnie Raspberry Pi; telefon jako etap trzeci.
**Ustalenia sprzętowe (sprawdzone):** Moto G8 = `rav`, SoC Snapdragon 665 (SM6125), GPU Adreno 610.
**Portu w pmaports nie ma**, ale SM6125 jest w mainline (`sm6125.dtsi`) razem z urządzeniami
`pdx201`, `ginkgo`, `laurel-sprout`, `willow`. Port `rav` = napisanie device tree — **osobny projekt
niż shell**, nie wiązać z jego harmonogramem.
**Konsekwencje wbudowywane od razu** (tanie teraz, drogie później):
- `wl_touch` jako osobna ścieżka wejścia, nie emulacja myszy (M2);
- cele dotykowe ≥ 48 px logicznych; żadnej funkcji dostępnej wyłącznie po najechaniu;
  dotykowe odpowiedniki `Super+←/→` i `Super+D` (M3);
- transformacja wyjścia (obrót) i skala jako pola modelu wyjścia od M1;
- `fractional-scale-v1` — skale telefonowe rzadko są całkowite.
**Rekomendacja sprzętowa:** przed `rav` weryfikować na **Xiaomi Redmi Note 8T (`willow`)** —
port w pmaports istnieje, a SoC i GPU są identyczne jak w Moto G8. Wcześniej wystarczy tani
ekran dotykowy USB do PC.
**Wzmacnia D-001:** Adreno 610 ma `freedreno` (GLES do 3.2), ale Turnip/Vulkan dla 610 jest
niepewny — renderer GLES2 jest jedyną bezpieczną drogą także na telefonie.
**Odniesienie:** `03-cel-telefon.md`

## D-025 — Model okien: kafelkowanie, bez swobodnego pozycjonowania
**Status:** ✅ **PRZYJĘTA** (2026-07-30) — **blokuje M2**
**Decyzja:** Okna aplikacji **nie nakładają się i nie są przesuwane myszą**. Dzielą obszar aplikacji
automatycznie. Brak ramek do chwytania, brak uchwytów zmiany rozmiaru, brak kolejności nakładania.
**Uzasadnienie:** Przewidywalność, brak zarządzania oknami jako czynności, dobre przeciąganie
plików między dwoma programami, a na ekranie 6,4" swobodne okna i tak byłyby nieużywalne.
Spójne z minimalizmem specyfikacji.

### Reguły wynikające z decyzji

**Kafelkowanie nie znaczy „wszystkie okna widoczne naraz".** Przy 720 px w pionie dwa kafelki mają
po 360 px — to działa; trzy mają po 240 px — to nie działa. Dlatego:
- **Limit jednocześnie kafelkowanych okien** (proponowane: 2 na telefonie, 2–3 na PC).
  Pozostałe okna żyją dalej, są na dolnym pasku i wchodzą na miejsce wybranego kafelka po kliknięciu.
- Nowe okno: zajmuje wolny kafelek, a gdy limit osiągnięty — **zastępuje kafelek z fokusem**,
  wypchnięte okno wraca na dolny pasek.

**Kierunek podziału wzdłuż dłuższej osi ekranu:**
- pion (telefon portretowo) → podział **poziomy**, kafelki jeden nad drugim (pełna szerokość
  dla list plików);
- poziom (PC, telefon obrócony) → podział **pionowy**, kafelki obok siebie.

**Suwak podziału jest dozwolony.** Przeciąganie **granicy między kafelkami** to nie to samo,
co swobodna zmiana rozmiaru okna — jest tanie, oczekiwane i idiomatyczne dla tego projektu
(specyfikacja ma już „Resize — przeciąganie krawędzi karty"). Proporcja zapamiętywana.

**Co NIE podlega kafelkowaniu** (najczęstsza przyczyna, dla której naiwne kafelkowanie jest
nieużywalne):
- okna modalne i dialogowe (`xdg_toplevel` z rodzicem), okna wyboru pliku → **pływające,
  wyśrodkowane nad rodzicem**;
- popupy, menu, podpowiedzi (`xdg_popup`) → pozycjonowane przez klienta, nigdy kafelkowane;
- tryb pełnoekranowy (wideo, gry, Moonlight) → **wychodzi poza kafelkowanie** na cały obszar.

**Minimalne rozmiary klientów muszą być respektowane** — jeśli kafelek zszedłby poniżej
`set_min_size` aplikacji, nie kafelkujemy jej, tylko zostawiamy na całym obszarze.

**Skróty klawiszowe** (przy klawiaturze Bluetooth) — bez kolizji z nawigacją kart z D-007:
`Super+←/→` = zmiana karty · `Super+Shift+←/→` = przeniesienie okna między kafelkami ·
`Super+Shift+Q` = zamknięcie kafelka. Na dotyku wymagane odpowiedniki gestowe.

### ⚠ Konsekwencja wymagająca uwagi: przeciąganie plików między oknami

Kafelkowanie wybrano m.in. dla wygodnego przeciągania plików między dwoma programami — ale
przeciąganie **między osobnymi oknami** wymaga pełnego DnD przez `wl_data_device`, które jest
wyraźnie trudniejsze niż sam schowek i niż przeciąganie wewnątrz jednej aplikacji.
Wcześniejsza rekomendacja z `00-przeglad-specyfikacji.md` §5.5 („w v1 DnD tylko wewnątrz menedżera
plików") **przestaje być wystarczająca** — bez DnD międzyklienckiego główny zysk z kafelkowania
nie istnieje.
**Rozstrzygnięcie:** DnD międzykliencki wchodzi do **M6** (nie „później"), a dwupanelowy tryb
w samym menedżerze plików **zostaje w zakresie** — daje przeciąganie za darmo w obrębie jednego
procesu i jest zabezpieczeniem na wypadek, gdyby DnD międzykliencki okazał się kosztowniejszy,
niż zakładamy.

### Powiązania
- **Zgodne z D-003** (Model A): slider pozostaje warstwą pulpitu, okna go zakrywają.
  Kafelkowanie dotyczy wyłącznie tego, jak okna dzielą się obszarem między sobą.
- **Zgodne z D-009**: przypięta karta rezerwuje przestrzeń, kafelkowanie dzieli to, co zostaje.
- **Ryzyko przy D-013/M5 (XWayland):** aplikacje X11 często zakładają swobodne okna i same ustawiają
  geometrię; okna override-redirect i dialogi wymagają osobnej obsługi. Do przetestowania na
  konkretnych programach w M5.
**Odniesienie:** `00-przeglad-specyfikacji.md` §4.1, §5.5

## D-022 — Tryb wskaźnika (wirtualny gładzik) — telefon jako komputer
**Status:** ✅ **PRZYJĘTA** (2026-07-30)
**Kontekst:** Cel to nie „shell mobilny", tylko **zrobienie z telefonu komputera**. Wzorzec
z aplikacji RDP na telefonie: przesuwanie palcem porusza kursorem, palec działa jak gładzik.
Rozwiązuje realny problem — aplikacje desktopowe (zwłaszcza przez XWayland) mają małe cele,
reagują na najechanie, mają prawy przycisk i przeciąganie; dotykiem bezpośrednim są nieużywalne.
**Decyzja — podział ról:**
- **Własny UI GostUI** → dotyk bezpośredni (duże cele, przesunięcia, długie przytrzymanie).
- **Obce aplikacje desktopowe** → tryb wskaźnika, włączany **świadomie przez użytkownika**.
- **Mysz i klawiatura Bluetooth** → normalne `wl_pointer` / `wl_keyboard`.
**Wymagania:**
- Ruch **względny** (gładzik), nie bezwzględny — podniesienie i postawienie palca nie przenosi kursora.
- **Kompozytor rysuje kursor**; kursor zostaje na miejscu po podniesieniu palca, dzięki czemu
  **działa najechanie**, niedostępne przy dotyku bezpośrednim.
- Pełny zestaw akcji: stuknięcie = LPM, dwa palce/przytrzymanie = PPM, przytrzymanie i ruch = przeciąganie
  (z **blokadą przeciągania** dla długich operacji), dwa palce = przewijanie.
- Konfigurowalna czułość i przyspieszenie.
- `relative-pointer-v1` i `pointer-constraints-v1` przechodzą z „dla gier" do rdzenia wejścia.
**Doprecyzowuje D-020:** zakaz „emulacji myszy" dotyczył *niejawnego* zastępowania `wl_touch`
przez `wl_pointer`. Tryb wskaźnika jest czymś innym — jawną, przełączaną ścieżką, która wymaga
właśnie tego rozdzielenia, żeby dała się czysto zaimplementować.
**Konsekwencje:** XWayland (M5) awansuje z „odblokowania Warstwy 2" do warunku podstawowego
zastosowania systemu · dolny pasek zyskuje na znaczeniu (przełączanie okien staje się codzienne),
więc jego automatyczne chowanie na telefonie jest wątpliwe · system musi działać dobrze
**zarówno** z klawiaturą Bluetooth, jak i bez niej.
**Odniesienie:** `03-cel-telefon.md` §6

## D-023 — Zakres portu telefonowego: bez aparatu, modemu i GPS
**Status:** ✅ **PRZYJĘTA** (2026-07-30)
**Decyzja:** Aparat, modem/SIM i GPS są **świadomie wyłączone**. Zostają **Wi-Fi i Bluetooth**.
**Znaczenie:** To najważniejsza dobra wiadomość dla wykonalności portu. Przy mainlinowaniu telefonu
najwięcej pracy i porażek generują dokładnie modem (QMI, firmware), aparat (CAMSS, ISP) i GPS.
Skreślenie ich usuwa większość ryzyka — **zakres portu `rav` jest istotnie mniejszy, niż
sugerowała pierwotna ocena w D-020.**
**Zostaje:** wyświetlacz DSI (panel w device tree — najwięcej dłubania z tego, co zostało), dotyk,
GPU Adreno 610, Wi-Fi, **Bluetooth**, dźwięk, bateria i ładowanie, USB, uśpienie, pamięć masowa.
**Bluetooth awansuje do funkcji krytycznej** — bez niego nie ma myszy ani klawiatury, czyli nie ma
komputera (D-022).
**Odniesienie:** `03-cel-telefon.md` §7

## D-024 — System zapisywalny, z normalną instalacją programów
**Status:** ✅ **PRZYJĘTA** (2026-07-30)
**Decyzja:** Żadnego systemu tylko-do-odczytu ani obrazu niezmiennego (immutable). Normalny
zapisywalny system z menedżerem pakietów.
**Zgodność:** postmarketOS **nie jest** systemem niezmiennym — ma zwykły zapisywalny rootfs
i normalny menedżer pakietów. Nic nie trzeba obchodzić.
**Doprecyzowanie:** partycja `/vendor` z firmware'em (Wi-Fi, BT, GPU) pozostaje tylko-do-odczytu —
to konieczne i normalne, nie ogranicza instalowania programów. Pamięć Moto G8 (64 GB) mieści pełny
system z aplikacjami desktopowymi.
**Wzmacnia D-022:** skoro instaluje się normalne programy, większość z nich będzie aplikacjami X11 →
XWayland jest warunkiem podstawowego zastosowania.
**Odniesienie:** `03-cel-telefon.md` §8

## D-021 — Animacja dekoracyjna a manipulacja bezpośrednia
**Status:** ✅ **PRZYJĘTA** (2026-07-30) — doprecyzowanie „braku animacji" ze specyfikacji
**Kontekst:** Specyfikacja zakazuje animacji przejść (snap). Na dotyku przesunięcie, przy którym
zawartość nie podąża za palcem, sprawia wrażenie zepsutego — brak informacji zwrotnej, czy gest
został zarejestrowany.
**Decyzja:**
- **Animacja dekoracyjna** (przenikanie, odbicia, efekty, animowane przejścia przy klawiaturze
  i myszy) — **nadal zakazana**, zgodnie ze specyfikacją.
- **Manipulacja bezpośrednia** (karta podąża za palcem 1:1, po puszczeniu wskakuje na miejsce) —
  **wymagana przy dotyku**.
**Zgodność z celem wydajnościowym:** renderowanie zachodzi tylko w trakcie ruchu palca, czyli gdy
coś faktycznie się dzieje. Próg „0 klatek na 10 s w spoczynku" (D-017) pozostaje nienaruszony.
**Odniesienie:** `03-cel-telefon.md` §3.2

## D-019 — Historia repozytorium po zmianie kierunku projektu
**Status:** ✅ **PRZYJĘTA** (2026-07-30)
**Kontekst:** `ventura85/GOST-OS` zawierał 22 commity poprzedniego projektu — dystrybucji Debiana
budowanej przez `live-build` (XFCE + motyw WhiteSur). Nowy projekt to shell pisany od zera;
żaden kod się nie przenosi.
**Decyzja:** Stan sprzed zmiany oznaczony tagiem **`v0-live-build-iso`** i wypchnięty na GitHub;
gałęzie boczne (`ci/bookworm-iso`, `ci/trixie-iso`, `fix/live-build-fixes`) zostawione.
`main` wystartował od nowej, jednocommitowej historii.
**Przeniesione:** wyłącznie zasoby graficzne → `resources/branding/` (6 plików).
**Pominięte:** motyw ikon WhiteSur (3744 pliki, 24 MB) — motyw firm trzecich, docelowo zależność
instalowana z pakietu, nie pliki wersjonowane w repo.

**Uzupełnienie (2026-07-30):** repozytorium przestawione na **prywatne**, a stare gałęzie
boczne **usunięte** — zdalnie zostaje sam `main`. Historia nie została jednak utracona:
`ci/bookworm-iso` i `ci/trixie-iso` były w całości zawarte w tagu `v0-live-build-iso`,
a `fix/live-build-fixes` miała **44 commity spoza tagu** (łatki źródeł trixie, hook instalujący
motyw, porządki w liście pakietów), więc przed skasowaniem została zarchiwizowana własnym
tagiem **`v0-live-build-fixes`**.

**Zasada, którą to ustanawia:** gałąź kasuje się dopiero wtedy, gdy jej commity są osiągalne
z jakiegoś tagu. Sprawdzenie kosztuje jedno polecenie:
`git rev-list --count <tag>..origin/<galaz>` — wynik różny od zera znaczy „najpierw otaguj".

**Licencja a prywatność:** GPL-3.0 zobowiązuje do udostępnienia źródeł dopiero przy
rozpowszechnianiu binariów. Prywatne repozytorium i GPL-3.0 nie kolidują, dopóki nikomu
nie przekazujemy zbudowanego systemu.

**Odtworzenie starego projektu:** `git checkout v0-live-build-iso` (stan ISO) albo
`git checkout v0-live-build-fixes` (z poprawkami live-build).

## D-015 — Warsztat: Ubuntu tutaj, Debian w maszynie wirtualnej
**Status:** ✅ **PRZYJĘTA** (2026-08-01 — domknięcie; rekomendacja z 2026-07-30 obowiązywała
w praktyce od M0 i się sprawdziła)
**Decyzja:** Kod pisany i budowany na tej stacji (Ubuntu 24.04 + XFCE/X11). Debian minimalny
weryfikowany w QEMU/KVM (Tier 3) — tam testowane wdrożenie, brak DE, sesja, pakiet.
Nie instalować Debiana na goliźnie tej maszyny.
**Uzupełnienie z D-034:** rozróżnienie „warsztat vs. cel wdrożeniowy" pozostaje w mocy — Debian jest
celem, Ubuntu wyłącznie stacją roboczą. Obraz budowania (`Dockerfile`) stoi na trixie właśnie
dlatego, że pilnuje celu, nie warsztatu.
**Odniesienie:** `01-strategia-dev-test.md` §2.1, D-034

## D-016 — Granica `gostui-core` ↔ kompozytor
**Status:** ✅ **PRZYJĘTA** (2026-07-30) — decyzja inżynierska, bez sensownej alternatywy
**Decyzja:** Cała logika (model kart, konfiguracja, `.desktop`, VFS, obliczenia layoutu, mapa
skrótów) w crate'ach **bez zależności od `smithay` i `wayland-*`**. Kompozytor wyłącznie tłumaczy
zdarzenia protokołu na wywołania core i rysuje jego stan.
**Uzasadnienie:** Umożliwia testowanie ~60% projektu przez `cargo test`, bez ekranu i bez GPU.
Alternatywa (logika wpleciona w kompozytor) oznacza, że każdy test wymaga działającego środowiska
graficznego — w praktyce brak testów.
**Test praktyczny:** jeśli do przetestowania fragmentu logiki potrzebny jest działający kompozytor,
ten fragment jest w złym miejscu.
**Wzmocnione przez D-003:** skoro karty nie znają okien, model kart jest w całości testowalny
w izolacji.
**Odniesienie:** `01-strategia-dev-test.md` §2.2

## D-017 — Progi wydajności jako kryterium akceptacji
**Status:** ⚠️ **ZAOSTRZONA** przez D-027 (2026-07-30), **uszczegółowiona** przez D-029 (2026-07-31)
**Propozycja pierwotna:** 0 klatek na 10 s w spoczynku · CPU < 1% w spoczynku · RSS < 120 MB
(kompozytor + paski + slider) · start do slidera < 1 s · zmiana karty < 16 ms · 24 h soak
bez wzrostu RSS.
**Zaostrzenie:** budżet RSS **120 MB → 80 MB** i staje się testem, który psuje build, nie liczbą
mierzoną ręcznie. Uzasadnienie w D-027: na maszynie z 2 GB RAM każde 40 MB shella to 40 MB
zabrane przeglądarce użytkownika.
**Uszczegółowienie (D-029):** 80 MB dotyczy ścieżki Pixman jako całego procesu; na ścieżce GLES2
próg wynosi 160 MB, bo obejmuje sterownik GPU. Powód w D-028: sam Mesa/radeonsi bierze ~79 MB.
**Odniesienie:** §6.2, D-027, D-029

## D-027 — Stary komputer jako główny cel wdrożeniowy
**Status:** ✅ **PRZYJĘTA** (2026-07-30)
**Kontekst:** Użytkownik: „telefon to marzenie, RPi to konieczność, ale ogólnie chciałbym bardzo
lekki system na stary komputer — żeby ten system nie miał wymagań". To przesuwa środek ciężkości:
stary PC przestaje być efektem ubocznym decyzji podjętych dla telefonu, a staje się **celem,
z którego wynikają progi**.
**Decyzja:** GostUI ma być mierzalnie lekki na sprzęcie sprzed dekady i więcej. Progi z D-017
obowiązują **przy narzuconych ograniczeniach**, nie na stacji deweloperskiej.

**Doprecyzowanie „bez wymagań" — bo dosłownie wzięte jest niewykonalne.** Dolnej granicy nie
wyznacza GostUI, tylko programy, które użytkownik uruchamia: shell zmieści się w 80 MB,
ale Firefox z kilkoma kartami weźmie 700 MB niezależnie od tego, co zrobimy. Właściwe
sformułowanie celu brzmi więc: **koszt własny GostUI ma być pomijalny, żeby cała pamięć starej
maszyny została dla programów użytkownika.** To jest osiągalne i sprawdzalne; „zero wymagań"
nie jest ani jednym, ani drugim.

**Realistyczna podłoga sprzętowa:**
| | Minimum | Komfortowo |
|---|---|---|
| CPU | dwurdzeniowy x86_64 (Core 2 Duo / Athlon 64 X2, 2006+) | cokolwiek z 2012+ |
| RAM | 1 GB (shell + terminal + menedżer plików) | 2 GB (z przeglądarką) |
| GPU | żaden — ścieżka Pixman | Intel HD / GMA X3100+ z GLES2 |

**Ograniczenie do sprawdzenia przed obietnicą 32-bitową:** Debian 13 (trixie) nie ma już instalatora
ani jądra dla `i386` — `i386` istnieje wyłącznie jako architektura pomocnicza (multiarch, biblioteki
32-bitowe). Maszyny wyłącznie 32-bitowe (Pentium 4, Athlon XP) wypadają więc z zakresu **nie przez
naszą decyzję, tylko przez dystrybucję**. Rust ma `i686-unknown-linux-gnu` w tier 1, więc gdyby
kiedyś doszła dystrybucja 32-bitowa, kod się skompiluje. **Nie obiecywać tego przed weryfikacją.**

**Co z tego wynika dla kodu — nic nowego, tylko wzmocnienie istniejących zasad:**
1. **Ścieżka Pixman jest równorzędna, nie awaryjna.** Na maszynie bez GPU renderer Pixman bije
   GLES2 emulowane przez `llvmpipe`. Obie ścieżki muszą działać na każdym etapie (D-001).
2. **Rysowanie tylko uszkodzonych regionów przestaje być optymalizacją.** Pełne odświeżenie
   1920×1080 w software to ~8 MB zapisu do pamięci na klatkę — na starym kontrolerze pamięci
   to różnica między „działa" a „nie da się używać".
3. **Zero renderowania w spoczynku ma teraz drugie uzasadnienie** poza baterią telefonu:
   stary procesor grzeje się i głośno chłodzi, gdy shell odrysowuje ekran bez powodu.
4. **Budżet RSS jest testem, nie pomiarem.** Regresja lekkości ma psuć build, a nie zostać
   odkryta rok później.

**Dobra wiadomość porządkująca całość:** ten cel **nie dodaje osobnego etapu ani sprzętu
do zdobycia**. RPi3 (Cortex-A53 1,2 GHz, VideoCore IV, 1 GB) jest ostrzejszym progiem niż typowy
stary pecet, a QEMU i cgroup v2 pozwalają symulować ograniczenia na tej stacji już dziś.
Trzy cele — telefon, RPi, stary PC — mają **wspólne wymaganie**: mało pamięci, brak mocnego GPU,
zakaz marnowania cykli. Jedna praca zaspokaja wszystkie trzy.
**Odniesienie:** `01-strategia-dev-test.md` §3.7

## D-028 — smithay 0.7 z minimalnym zestawem cech (i co z tego wynika dla Pixmana)
**Status:** ✅ **PRZYJĘTA** w części „wersja i cechy" (2026-07-31) · ✅ **PRZYJĘTA** w części
„jak podać Pixmana zagnieżdżonego" (2026-08-02) — **warunek spełniony**, patrz dopisek na końcu.

**Decyzja:** `smithay = { version = "0.7.0", default-features = false, features = ["backend_winit"] }`,
wyłącznie w `gostui-compositor` (D-016).

**Dlaczego bez domyślnych cech:** domyślny zestaw smithaya włącza DRM, GBM, libinput, libseat,
Vulkan, XWayland i wielorenderer. Każda z tych cech to biblioteka systemowa, którą musiałoby nieść
CI i kontener, żeby skompilować krok, który jeszcze nie istnieje. Cechy dochodzą etapami:
`backend_udev` + `backend_session_libseat` w M4, `xwayland` w M5.
Uwaga: `backend_winit` **samo** wciąga `backend_egl` i `renderer_gl` — nie da się mieć okna
zagnieżdżonego bez GLES-a.

**Co zweryfikowano na stacji (2026-07-31):** smithay 0.7.0 (MIT, MSRV 1.80.1) kompiluje się
na Ruście 1.97.1; ciągnie winit 0.30, calloop 0.14, wayland-server 0.31. Okno się otwiera
(EGL 1.5 / PLATFORM_X11_KHR, GLES 3.2, radeonsi na Vega 11) i zamyka z kodem 0.

**Zysk, którego nie było w planie:** `WinitEventLoop` implementuje `calloop::EventSource`.
Pętla zdarzeń śpi w `poll` i budzi się na zdarzeniu — „zero renderowania w spoczynku" jest więc
zgodne z biblioteką, a nie wbrew niej. Zweryfikowane: budżet 100 klatek nie został wyczerpany
przez 6 s stania z otwartym oknem (narysowana 1 klatka, ta początkowa).

**Pułapka, która zmienia kolejność kroku 3 M1:** `winit::init::<R>()` wymaga
`R: From<GlesRenderer> + Bind<EGLSurface>`. **Backend `winit` jest z definicji GLES-owy** —
„Pixman najpierw" nie da się zrobić przez okno zagnieżdżone. Ścieżki wyjścia:
1. **`backend_x11`** dla ścieżki Pixman: smithay alokuje bufory przez GBM, `PixmanRenderer`
   wiąże liniowy dmabuf. Prawdziwa ścieżka CPU zagnieżdżona, koszt: drugi backend zagnieżdżony
   plus zależności `gbm`/`drm` w buildzie od M1 zamiast od M4.
2. **CPU do tekstury:** rysujemy rasteryzerem z `gostui-render` i wgrywamy wynik jako teksturę
   GLES. Kilka linijek, ale ścieżka Pixman zagnieżdżona nie jest wtedy prawdziwą ścieżką Pixman —
   prawdziwa pojawia się dopiero na DRM z dumb bufferami (M4).

**Wybrano (2)** — decyzja właściciela repozytorium, 2026-07-31 — pod warunkiem, że ścieżka CPU
zostaje **pokryta testami golden PNG** (deterministyczna, bez ekranu i GPU). Ryzyko gnicia ścieżki
CPU do M4 kontroluje test obrazu, nie backend zagnieżdżony. Alternatywa (1) kupowała wierność
kosztem zależności systemowych na cztery etapy do przodu.
**Konsekwencja dla kodu:** rysowanie rozdziela się na **listę wyświetlania** (prostokąty
w jednostkach logicznych, liczone raz, wspólne dla obu ścieżek) i jej rasteryzację —
`gostui-render` na CPU albo `draw_solid` w GLES2. Bez tego rozdziału obie ścieżki rozjeżdżają się
po pierwszym tygodniu.
**Zweryfikowane po implementacji (2026-07-31):** zrzuty okna z `--renderer gles2` i `--renderer
pixman` (1360×850) są identyczne co do piksela (`compare -metric AE` = 0). Rozdział działa —
obie ścieżki wykonują tę samą listę, zamiast liczyć geometrię osobno.

**Pomiar, który psuje spokój (D-027, D-017):** `--release`, okno zagnieżdżone, jedna klatka,
nic poza tłem: **RSS 90,8 MB**. Ta sama binarka bez GL: 3 MB (`--help`), 11,5 MB (`--png`,
dwa pełne canvasy 1920×1080 + PNG). Różnicę — około 79 MB — bierze sterownik Mesa/radeonsi
z EGL-em, zanim narysowaliśmy cokolwiek własnego. Budżet 80 MB z D-017/D-027 jest więc
**już przekroczony przez sam sterownik GPU**, a nie przez nasz kod. Rozstrzygnięcie: **D-029**.

**Zależność transitywna do odnotowania:** smithay ciągnie `cgmath` (RUSTSEC-2026-0196,
*unmaintained*, bez wersji naprawionej). Wpisane do `deny.toml` z uzasadnieniem — to notka
o utrzymaniu, nie podatność, a crate liczy macierze i nie dotyka danych klienta.

**Warunek spełniony — dopisane 2026-08-02.** Wybór wariantu (2) był warunkowy: ścieżka CPU miała
zostać pokryta testami golden PNG, bo bez nich nikt na nią nie patrzy do M4. Przez trzy dni tego
warunku nie było, i **kosztowało to dokładnie to, przed czym miał chronić**: ikona Menu Start
zmieniła wygląd powłoki, a cały zestaw testów przeszedł bez uwag.

Teraz `crates/gostui-render/tests/golden/` trzyma cztery sceny porównywane co do piksela przez
zwykłe `cargo test`: monitor, telefon w pionie przy skali 2, pasek za wąski na wszystkie
elementy, i pusta sesja. **Piksele, nie zrzut listy wyświetlania** — lista jest wspólna dla obu
ścieżek, więc usterka w rasteryzerze CPU zostawia ją identyczną; rasteryzer widać tylko
w pikselach, a to on jest tu chroniony.

Sceny nie zawierają tekstu, bo zegar jest jedynym tekstem powłoki i `clock: None` usuwa wszystkie
glify. To nie jest uproszczenie, tylko warunek odtwarzalności: glify przywiązałyby wzorce do
wersji czcionek maszyny, która je narysowała. **Sprawdzone: pliki z tej stacji zgadzają się co do
piksela na maszynie CI** — czyli rasteryzer jest deterministyczny nie tylko między przebiegami,
ale i między maszynami, co jest cichym założeniem całego tego wariantu. Koszt w CI: `check`
z ~57 s do ~1 min 36 s.

**Uwaga do historii:** commit wprowadzający te testy powołuje się na „D-010" zamiast na tę
decyzję. Numer jest błędny (D-010 dotyczy wielu wyjść), a historia jest publiczna, więc korekta
mieszka tutaj i w komentarzu modułu `golden.rs`.
**Odniesienie:** `01-strategia-dev-test.md` §4, D-001, D-016, D-027, D-029

## D-029 — Budżet RSS osobno dla ścieżki Pixman i dla GLES2
**Status:** ✅ **PRZYJĘTA** (2026-07-31) — uszczegóławia D-017/D-027, nie zastępuje ich.
Pozostałe progi z D-017 (0 klatek w spoczynku, CPU < 1%, start < 1 s, zmiana karty < 16 ms,
24 h soak) obowiązują bez zmian.

**Problem:** pomiar z D-028 pokazał, że sterownik GPU (Mesa/radeonsi + EGL) bierze ~79 MB RSS,
zanim kompozytor narysuje pierwszy prostokąt. Jeden próg 80 MB dla obu ścieżek oznaczałby test,
który na ścieżce GLES2 nie może przejść z powodów niezależnych od naszego kodu.

**Decyzja — dwa progi, oba mierzone na `--release`, na jednej klatce, bez klientów:**
| Ścieżka | Próg RSS | Co obejmuje |
|---|---|---|
| Pixman (CPU) | **≤ 80 MB** | cały proces — na starym pececie bez GPU nie ma czego odejmować |
| GLES2 | **≤ 160 MB** | cały proces **razem ze sterownikiem** |

**Który próg jest naprawdę nasz:** Pixman. Liczba GLES2 jest przypięta do sterownika i sprzętu
(radeonsi ≠ Adreno ≠ `llvmpipe`), więc na innej maszynie trzeba ją zmierzyć od nowa i nie należy
z niej wyciągać wniosków o naszym kodzie. Regresję lekkości śledzimy po liczbie z Pixmana;
próg GLES2 jest siatką bezpieczeństwa łapiącą wyciek tekstur i buforów.

**Punkt wyjścia do porównań (2026-07-31, Vega 11, jedna klatka, samo tło):** GLES2 90,8 MB.
Binarka bez kontekstu GL: 3,0 MB (`--help`), 11,5 MB (`--png`, dwa canvasy 1920×1080).

**Stan wdrożenia (2026-08-01) — progi są ustalone, testu nie ma.** Nic w kodzie ani w CI nie
odczytuje RSS; kontrola jest ręczna (`/usr/bin/time -v`). Powtórzony pomiar zgadza się
z powyższymi: `--png` 11,5 MB, okno Pixman 95,5 MB przy `--frames 1`.
**Kiedy test powstanie:** przy kroku 5 M1, nie wcześniej. Dziś zmierzyłby 11 MB przy progu 80
i nigdy by nie zapłonął — czcionki (`cosmic-text` z atlasem glifów, 1–4 MB) są pierwszym
realnym przyrostem, a kafle żywe z D-033 pierwszym, który może próg naruszyć.
**Czego test ma dotyczyć:** liczby z Pixmana, zgodnie z akapitem „Który próg jest naprawdę nasz".
Próg GLES2 mierzony na tej stacji jest przypięty do radeonsi i nie przenosi się na inną maszynę.
**Odniesienie:** D-017, D-027, D-028, D-033, `01-strategia-dev-test.md` §3.7

## D-018 — Dostępność poza zakresem
**Status:** OTWARTA
**Decyzja proponowana:** AT-SPI poza zakresem v1 (świadomie, nie przez przemilczenie).
Zachować to, co tanie: pełna obsługa klawiatury, konfigurowalny rozmiar czcionki.
**Odniesienie:** §6.3

## D-026 — Kryterium doboru telefonu: wyjście obrazu na monitor
**Status:** ✅ **PRZYJĘTA** (2026-07-30) — rewiduje D-010, doprecyzowuje D-020
**Kontekst:** Użytkownik: „muszę znaleźć telefon z wyjściem pod monitor". To zmienia sens hasła
„telefon jako komputer": nie ekran 6,4" z peryferiami Bluetooth (jak przy Moto G8), tylko
**telefon w stacji dokującej: monitor, klawiatura, mysz** — a telefon staje się jednostką centralną.
**Konsekwencje projektowe (nie „kiedyś", tylko od M1):**
1. **Model wyjść jest kolekcją**, każde wyjście ma własną rozdzielczość, skalę i transformację.
   Kod nie może zakładać „jedynego wyjścia" nigdzie poza warstwą prezentacji.
2. **Gorące podłączenie i odłączenie wyjścia** — okna i kafelki muszą przeżyć zniknięcie wyjścia,
   na którym stały. To najczęstsze miejsce paniki kompozytora; przy stacji dokującej zdarza się
   codziennie, nie okazjonalnie.
3. **Layout kafelków (D-025) liczy się per wyjście.** Podział wzdłuż dłuższej osi daje na telefonie
   kafelki jeden nad drugim, a na monitorze obok siebie — **jednocześnie**, na tej samej sesji.
4. **Limit kafelków jest funkcją wyjścia, nie urządzenia** (2 na ekranie telefonu, 2–3 na monitorze).
5. **Tryb wskaźnika (D-022) nie znika po zadokowaniu** — ekran telefonu staje się wtedy gładzikiem,
   co jest jego najbardziej naturalnym zastosowaniem.
**Sprzęt — kandydaci sprawdzeni, nie założeni:** wyjście obrazu przez USB-C ma na telefonach
z Androida praktycznie tylko **DisplayPort alt mode**, obecny głównie w rodzinie **SDM845**.
Z portami w pmaports w gałęzi **`community`** (dojrzalszej niż `testing`, w której siedzi `willow`):

| Telefon | Nazwa kodowa | SoC | pmaports | DP alt mode |
|---|---|---|---|---|
| OnePlus 6 | `oneplus-enchilada` | SDM845 | `community` | tak |
| OnePlus 6T | `oneplus-fajita` | SDM845 | `community` | tak |
| Pocophone F1 | `xiaomi-beryllium` | SDM845 | `community` | tak |
| SHIFT6mq | `shift-axolotl` | SDM845 | `community` | tak |

Sony Xperia z rodziny `tama` (`akari`, `akatsuki`) są w mainline (DTS istnieje), ale portu
w pmaports brak. Sterownik `drivers/usb/typec/altmodes/displayport.c` jest w mainline.
**Znaczenie dla harmonogramu:** takie urządzenie jest **łatwiejsze** niż Moto G8, nie trudniejsze —
port już istnieje i jest utrzymywany, więc etap „telefon" przestaje zawierać ukryty podprojekt
„napisz device tree dla `rav`". SDM845 ma też Adreno 630 z działającym Turnipem, ale **to niczego
nie zmienia w D-001** — GLES2 zostaje, bo musi działać także na RPi3 i na Adreno 610.
**Status Moto G8:** zostaje jako sprzęt, który użytkownik ma pod ręką — dobry do testów dotyku
i skalowania, nie do scenariusza dokowania.
**Odniesienie:** `03-cel-telefon.md` §2.1, §7.1

## D-030 — Orientacja pozioma jako domyślna na telefonie
**Status:** OTWARTA (rekomendacja: przyjąć) — doprecyzowuje D-020 i D-026
**Kontekst:** Użytkownik: „zmieniam podejście używania telefonu z | na -". Do tej pory milcząco
zakładaliśmy telefon trzymany pionowo, bo tak trzyma się telefon. Ale celem nie jest telefon,
tylko **komputer** (D-020, D-024) — a komputer ma ekran szerszy niż wyższy.

**Pomiar, nie przypuszczenie.** Wyjścia policzone przez `gostui_core::layout::tile()`
przy `Gaps { inner: 8, outer: 8 }` i paskach 48/48:

| Wyjście | Strefa aplikacji | Limit kafelków | Kafelek |
|---|---|---|---|
| monitor 1920×1080 | 1920×984 | 3 | 948×968 |
| telefon pion 360×780 | 360×684 | 2 | 344×330 |
| telefon poziom 780×360 | 780×264 | 2 | 378×248 |

**Decyzja:** poziom jest domyślną orientacją telefonu. Pion nadal musi działać — obrót siedzi
w `Transform` od początku — ale przestaje być przypadkiem projektowym, dla którego optymalizujemy.

**Uzasadnienie, z uczciwym rozliczeniem strat.** Wynik jest niejednoznaczny i to trzeba zapisać:
- **Dla dwóch okien pion wygrywa powierzchnią** (344×330 = 113 tys. jednostek² kontra
  378×248 = 94 tys.). Poziom tu przegrywa.
- **Dla jednego okna poziom wygrywa rozstrzygająco:** 780×264 kontra 360×684. Obce aplikacje
  desktopowe zakładają szerokość — paski narzędzi, menu, kolumny arkusza. Większość ma
  `set_min_size` szerszy niż 360, a zgodnie z D-025 aplikacji, która nie mieści się w kafelku,
  **nie kafelkujemy**. W pionie wypada więc znaczna część zakresu z D-024.
- Skoro sensem urządzenia jest obcy program desktopowy, a nie własny UI, drugi punkt bije pierwszy.

**Konsekwencje:**
1. **Architektura nie wymaga zmian** — i to jest sprawdzone, nie założone. `longer_axis()` sam
   przełączył podział na kafelki obok siebie, `is_portrait()` i `Transform` już istnieją. Zwrot
   z kosztu poniesionego w D-011 i D-026: zmiana zdania o orientacji kosztowała zero linii kodu.
2. **Paski przestają być tanie.** 48 + 48 = 96 jednostek to **27% ekranu 780×360**, a na monitorze
   9%. To jest realny koszt tej decyzji i rozwiązuje go D-032 (wysokości pasków jako dane motywu,
   z podłogą 48 tylko pod dotykiem — w stacji dokującej z myszą podłoga nie obowiązuje).
3. **Własna tablica kafli traci miejsca.** W pionie 4 kolumny × ~7 rzędów, w poziomie 8 × 2:
   z ~28 pól robi się 16. Kierunek przewijania tablicy zmienia się wraz z orientacją, co tworzy
   konflikt gestów rozstrzygnięty w D-031.
4. **Moto G8 pozostaje sprzętem do testów dotyku** (D-026) — decyzja nie zmienia doboru sprzętu.
**Odniesienie:** `03-cel-telefon.md`, podglądy `podglad/e-okna-telefon-{pion,poziom}.png`

## D-031 — Struktura środkowej strefy: pasek kart zamiast ramy karty
**Status:** ⛔ **ZASTĄPIONA przez D-046** (2026-08-03) — nigdy nie weszła do kodu.
Wpis zostaje w całości, bo jego pomiary są nadal prawdziwe; nieprawdziwe okazało się
**założenie wspólne dla niego i dla `gostos.md` §B**: że widoczna jest jedna karta naraz.
Przy kartach jako kolumnach skrawek powstaje sam i pasek nazw jest niepotrzebny.
Poprzedni status: OTWARTA (rekomendacja: przyjąć) — **odstępstwo od `gostos.md` §B**
**Kontekst:** Użytkownik: „chcę pozbycia się śmietnika pulpitu tradycyjnego i zastąpienia go
szybkim dostępem do funkcji i danych". Cztery warianty rozrysowane i obejrzane
(`podglad/{a,b,c,d}-*.png`) na trzech wyjściach.

**Decyzja:** środkowa strefa to **pasek nazw kart** (44 jednostki) plus **tablica kafli
zmiennej wielkości** na całą pozostałą przestrzeń. Bez ramy karty i bez skrawków sąsiednich kart.

**Co z tego wypada ze specyfikacji.** `gostos.md` §B wymaga: „sąsiednie karty częściowo widoczne
po bokach (skrawek jako wizualna podpowiedź, że jest kolejna karta)". Skrawek zostaje zastąpiony
paskiem nazw. **Nie jest to uproszczenie — pasek robi to samo lepiej:** mówi, ile jest kart
i gdzie jesteś, pozwala skoczyć do dowolnej karty jednym dotknięciem zamiast czterech przesunięć,
mieści się w progu dotykowym i kosztuje 44 jednostki zamiast obwódki z nagłówkiem. Skrawek 28
jednostek ciemnego tła na ciemnym tle jest w praktyce niewidoczny (`podglad/a-pelna-karta-*.png`).

**Rozstrzygający argument wyszedł z D-030, nie z estetyki:** w poziomie tablica ma 2 rzędy i musi
przewijać się w bok. Gdyby przesunięcie w bok zmieniało kartę, gest byłby zajęty. Pasek zdejmuje
ten konflikt: **przesunięcie = przewijanie zawartości karty, dotknięcie paska = zmiana karty.**

**Odrzucone warianty i dlaczego:**
- **A (karta pełnoekranowa, siatka jednolita)** — 28 identycznych kwadratów odtwarza szufladę
  aplikacji, czyli dokładnie ten śmietnik, od którego uciekamy. Żaden kafel nic nie mówi.
- **C (karta przypięta) — nie odrzucona, ograniczona.** Na monitorze panel z listą miejsc działa
  i realizuje „inteligentny pulpit [PLIKI]" ze specyfikacji. Na telefonie zabiera 2/5 ekranu.
  **Przypinanie (D-009) zostaje funkcją wyjść poziomych o dużej przestrzeni, nie telefonowych.**

**Konsekwencje:** liczba kolumn tablicy jest ustawieniem użytkownika, nie stałą (8 na monitorze
daje 32 pola do zapełnienia — dużo; 6 daje większe kafle). Pusta przestrzeń pod kaflami na dużym
wyjściu ma być jawną strefą upuszczania w trybie edycji, nie przypadkową dziurą.
**Odniesienie:** `gostos.md` §B (odstępstwo), D-008, D-009, D-021

## D-032 — Motyw jako dane: kolory, czcionki i rozmiary poza kodem
**Status:** ✅ **PRZYJĘTA** (2026-08-01) — struktura wdrożona w `gostui-core::theme`
**Kontekst:** Użytkownik: „chciałbym maksymalną możliwą personalizację kolorów, czcionek
i rozmiarów, żeby user mógł sobie to robić". Dziś `Palette` ma dziewięć kolorów zaszytych
w `gostui-render/src/paint.rs` plus pięć stałych `const` na rozmiary, a `Config` zna tylko
`font_size` i `icon_theme`.

**Decyzja:** motyw jest **danymi wczytywanymi z pliku**, nie kodem. Trzy warstwy:
1. **Kolory jako nazwane role**, nie surowe wartości: `desktop`, `bar`, `card`, `card_active`,
   `accent`, `text`, `text_dim`, `focus_ring`. Użytkownik podmienia rolę, nie 40 miejsc w kodzie.
   Nazwy po angielsku, jak wszystkie identyfikatory w tym repozytorium — pierwotny szkic tego
   wpisu podawał je po polsku, co było niezgodne z zasadą językową z `docs/04-zasady-pracy.md`.
2. **Rozmiary w tym samym pliku:** wysokości pasków, jednostka siatki kafli, odstępy, promienie,
   rozmiary czcionek per element (pasek ≠ podpis kafla).
3. **Czcionki przez fontconfig** — nazwa rodziny („Inter", „DejaVu Sans"), nie ścieżka do `.ttf`.
   `cosmic-text` (D-005) i tak odnajduje fonty przez fontconfig.

**Dwa ograniczenia, bez których personalizacja psuje system:**
- **`MIN_TOUCH_TARGET` = 48 jest podłogą, nie ustawieniem.** Motyw może paski pogrubić, nie może
  ich zwęzić poniżej progu na wyjściu obsługiwanym dotykiem. Inaczej jeden wpis w TOML daje system,
  którego nie da się kliknąć palcem. **Podłoga nie obowiązuje, gdy sesja ma wskaźnik** (mysz
  Bluetooth, stacja dokująca) — i to jest odpowiedź na koszt pasków z D-030.
- **Zły motyw nie może zabić kompozytora.** Wczytanie zawsze z odwrotem do motywu wbudowanego,
  błąd do logu. Awaria kompozytora to utrata wszystkich aplikacji użytkownika.

**Dlaczego teraz, a nie później:** kolory rozejdą się po rasteryzerze, warstwie tekstu, ikonach
i kaflach. Zebranie ich dziś to godziny, za trzy miesiące — dni i regresje. Struktura `Theme`
trafia do `gostui-core` (D-016): to dane layoutu, nie rysowania.

**Stan wdrożenia (2026-08-01):** `gostui-core::theme` zawiera `Rgba` (z parsowaniem `#rrggbb`),
`Palette`, `Metrics`, `Fonts`, `Theme`, `Pointing` i `Report`. 14 testów. `gostui-render`
nie ma już własnego `Rgba` ani `Palette` — reeksportuje je z core, żeby nie powstała druga
równoległa paleta. **Obraz sprawdzony: PNG-i przed i po są identyczne co do bajtu**
(SHA-256 `7ee7fe73…` dla monitora), więc przeniesienie niczego nie przemalowało.

**Czego jeszcze nie ma:**
- **wczytywania motywu z TOML** — `gostui-config` nadal zna tylko `font_size` i `icon_theme`.
  `gostui-core` celowo nie ma `serde`: mapowanie TOML → `Theme` należy do warstwy konfiguracji.
- ~~**rozmiarów starego slidera** (`CHIP_H`, `CARD_W`, `TILE`…)~~ — **zrobione 2026-08-03**:
  `CARD_W`, `CARD_GAP`, `TILE` i `TILE_GAP` zniknęły z `paint.rs` razem z układem pływających
  kart, a ich następcy (`card_width`, `card_gap`, `card_pad`, `card_header`) są w `Metrics`
  (D-046). `tab_strip` zmienił nazwę na `card_header`: pole opisywało pasek nazw z D-031,
  którego nie ma.
- **wykrywania `Pointing`** — kompozytor musi je ustalać z obecności `wl_pointer`; dziś nikt
  tej funkcji nie woła, jest tylko gotowa i przetestowana.
**Odniesienie:** D-005, D-011, D-016, D-020, D-030, D-031

## D-033 — Kafel żywy: dane na kaflu, odświeżanie wyłącznie na zdarzenie
**Status:** ✅ **PRZYJĘTA** (2026-08-04), **wdrożona w części** — patrz „Stan wdrożenia" niżej
**Kontekst:** Klasyczny pulpit jest śmietnikiem, bo jest katalogiem: rzeczy trafiają tam, bo muszą
gdzieś trafić, i nic o sobie nie mówią — ikona `raport.odt` wygląda tak samo w dniu utworzenia
i trzy lata później. Siatka identycznych skrótów odtwarza ten problem (patrz D-031, wariant A).

**Decyzja:** kafel ma dwa rodzaje. **Martwy** — skrót: ikona i podpis, kliknięcie uruchamia.
**Żywy** — pokazuje stan: „/ — 12 GB wolnego", „Pobrane — 3 nowe pliki", tytuł granego utworu;
kliknięcie wchodzi w szczegół. Rozmiar kafla na siatce (1×1, 2×1, 1×2, 2×2) wybiera użytkownik.

**Reguła odświeżania — to jest właściwa treść tej decyzji, nie sam wygląd kafla.**
Kafel żywy kusi do pętli odświeżającej, a to złamałoby wymaganie zera renderowania w spoczynku,
które ma teraz dwa uzasadnienia (bateria telefonu **i** stary procesor, D-027).
1. **Aktualizacja wyłącznie na zdarzenie ze źródła:** `inotify` dla katalogu, sygnał D-Bus dla
   odtwarzacza, `statvfs` przy zdarzeniu montowania.
2. **Nieliczne wyjątki, które muszą tykać** (zegar) przerysowują **wyłącznie własny prostokąt**
   i z najniższą częstotliwością, jaka wystarcza — zegar bez sekund odświeża się raz na minutę.
3. **Nigdy pętla po wszystkich kaflach.** Kafel niewidoczny (inna karta, przykryty oknem)
   nie odświeża się wcale — odsubskrybowuje źródło.

**Granica warstw (D-016):** źródło danych kafla to **logika, nie rysowanie**. Trafia do
`gostui-core` za traitem, testowalnym z atrapą źródła przez `cargo test`. Gdyby wylądowało
w rasteryzerze, mielibyśmy `inotify` w renderze — dokładnie ten błąd, przed którym D-016 chroni.

### Stan wdrożenia (2026-08-04)

**Weszło: podpis kafla martwego.** Nazwa siedzi wewnątrz kafla, przy dolnej krawędzi; kwadrat
ikony liczony wokół tego, co zostało (`shell::tile_face`). Kafel za mały na jedno i drugie
zachowuje znak, gubi nazwę. Tekst szerszy niż jego obszar jest skracany wielokropkiem, a
szerokość weszła do klucza cache'u tekstu.

**Nie weszło i w jakiej kolejności ma wejść:**
1. **Ikona kafla martwego** — wyszukiwanie zgodne z freedesktop Icon Theme Spec, rendering SVG,
   cache per rozmiar. To **nowa zależność**, więc przed kodem osobny wpis w tym rejestrze:
   który rasteryzer SVG, jaki limit cache'u i jaki test go pilnuje (D-039).
2. **Rozmiary kafla na siatce** (1×1, 2×1, 1×2, 2×2) wybierane przez użytkownika — dziś każdy
   kafel to jedna komórka.
3. **Kafel żywy**: źródło za traitem w core, atrapa w testach, subskrypcja **tylko dla kafli
   widocznych**, odświeżanie wyłącznie na zdarzenie. Wzorcem jest zegar i
   `Wall::until_next_minute` — nigdy pętla pytająca.

**Uwaga na przyszłość, wyniesiona z kroku 1:** wszystko, co dokłada tekst do środkowej strefy,
odbiera się złotym obrazom **danymi sceny** (skrót bez nazwy, `clock: None`), nigdy flagą
zmieniającą rysowanie. Flaga sprawiłaby, że obrazy pilnują wyglądu, którego nikt nie widzi.
**Odniesienie:** D-016, D-021, D-027, D-039, D-044, `gostos.md` §B

## D-034 — Debian jako podstawa systemu na PC i RPi
**Status:** ✅ **PRZYJĘTA** (2026-08-01) — spisanie założenia, które od początku było w specyfikacji

**Kontekst:** Wybór podstawy wracał w rozmowie jako pytanie otwarte (rozważane: Arch z `pacman`
i snapshotami btrfs, Alpine). W rzeczywistości **nie był otwarty** — `gostos.md` §Platformy mówi
„Debian (minimalny, bez DE)", obraz budowania w `zaleznosci.md` §Docker stoi na trixie, a kryterium
domknięcia M9 brzmi „`apt install ./gostui.deb` na czystej maszynie z Debianem minimalnym".
Wpis powstaje, żeby rejestr mówił to wprost, i żeby zamknąć rozważanie alternatyw.

**Decyzja:** Podstawą na PC x86_64 i Raspberry Pi jest **Debian** (obecnie trixie). Arch
i dystrybucje rolling release są **odrzucone**. Alpine pozostaje w grze **wyłącznie na telefonie**,
przez postmarketOS — patrz „Nadal otwarte" poz. 3 i `03-cel-telefon.md` §8.1.

**Uzasadnienie odrzucenia Archa** (jedyna alternatywa, która miała realny argument za sobą):
- Argument **za** był prawdziwy: `pacman` jest wyraźnie łatwiejszy do opakowania w GUI niż `apt` —
  przewidywalne wyjście, brak interaktywnego `debconf` w środku instalacji. Ale ten problem
  **rozwiązuje się raz**, w adapterze z D-037.
- Argument **przeciw** wynika prosto z D-027. Celem jest stary komputer i użytkownik, który nie
  zagląda do terminala. Arch oficjalnie nie wspiera częściowych aktualizacji — maszyna aktualizowana
  „jak się przypomni" potrafi się rozsypać przy jednym `pacman -Syu`. Snapshoty (btrfs + snapper
  + `grub-btrfs`) to ratują, ale **ratowanie to nie to samo co niepsucie**. Kruchość rolling release
  jest problemem u użytkownika na zawsze; trudność opakowania `apt` jest problemem u nas raz.
- Arch Linux ARM jest utrzymywany słabo, co uderzałoby w RPi (D-002).

**Czym jest „własny system oparty na Debianie" — i czym nie jest.** Nie zostajemy dystrybucją:
utrzymanie repozytorium z dziesiątkami tysięcy pakietów (przebudowa przy każdym CVE, dla każdej
architektury, w nieskończoność) jest poza zasięgiem tego projektu i byłoby złamaniem zasady
„nie wynajduj tego, co jest wystandaryzowane". Osiągalny zakres to cztery rzeczy:
1. **baza gotowa** — Debian trixie ze swoimi pakietami i poprawkami bezpieczeństwa, nietykany;
2. **jeden własny pakiet** — `gostui.deb`;
3. **własne repozytorium `apt`** tylko z nim — to katalog plików i podpis GPG, nie infrastruktura;
4. **własny obraz ISO** przez `live-build`: Debian minimalny + nasze repo + plik sesji + ustawienia
   domyślne, bez XFCE.

To **nie jest nowa praca do wymyślenia** — punkt 4 istnieje w historii tego repozytorium jako tag
`v0-live-build-iso` (D-019), tyle że wiózł wtedy XFCE z motywem WhiteSur. Mechanizm wraca przy
M9/M10 z naszym shellem w środku.

**Snapshoty:** warte zrobienia niezależnie od podstawy, ale przy Debianie stable przestają być
warunkiem używalności. Gdyby wchodziły: `/home` na osobnym subwolumenie **wyłączonym z rollbacku**
(inaczej cofnięcie systemu zjada dokumenty użytkownika), i pamiętać, że snapshot leży na tym samym
dysku — to nie jest kopia zapasowa.
**Odniesienie:** `gostos.md` §Platformy, `zaleznosci.md` §Docker, `01-strategia-dev-test.md` §2.1,
D-015, D-019, D-027, D-037

## D-035 — Wyjście wirtualne i pulpit zdalny jako konsekwencja modelu wyjść
**Status:** ✅ **PRZYJĘTA** (2026-08-01) — dotyczy kodu **od M1**, choć funkcja powstaje dużo później

**Kontekst:** Scenariusz użytkownika: „podłączam urządzenie do peceta i od razu widzę okienko
mojego systemu w Windows" — analogia do `scrcpy`. Analogia jest myląca i trzeba ją nazwać: `scrcpy`
działa, bo Android ma wbudowane API przechwytywania ekranu, sprzętowy enkoder i transport `adb`.
My nie jesteśmy stroną, która to konsumuje — **my jesteśmy stroną, która musi to udostępnić**.

**Decyzja — dwie części.**

**1. Nie piszemy własnego protokołu zdalnego pulpitu.** Udostępniamy przechwytywanie ekranu
(`wlr-screencopy-v1` albo ścieżka `xdg-desktop-portal` + PipeWire) i wstrzykiwanie wejścia
(wirtualna klawiatura i wskaźnik), a obraz w oknie na Windowsie robi **istniejący** serwer VNC/RDP.
**Do sprawdzenia, nie do założenia:** gotowe narzędzia z tej rodziny (`wayvnc`) są pisane pod
wlroots, a my jesteśmy na smithayu — protokoły są uniwersalne, przydatność samego narzędzia trzeba
zweryfikować, zanim wejdzie do planu.

**2. To, co ma znaczenie już teraz: `Output` nigdy nie zakłada fizycznego monitora.**
Sesja zdalna to z punktu widzenia kompozytora **wyjście bez sprzętu (headless) z wstrzykiwanym
wejściem** — nic więcej. Wymagania są więc **identyczne** z tymi, które D-026 nakłada dla stacji
dokującej: kolekcja wyjść, własna rozdzielczość, skala i transformacja per wyjście, dodawanie
i usuwanie wyjść w locie, przeżycie zniknięcia wyjścia, kafelkowanie liczone per wyjście.
Jeśli tego pilnujemy dla monitora HDMI, zdalny pulpit dostajemy niemal za darmo. Jeśli gdziekolwiek
w core przemyci się założenie „wyjście = podłączony ekran", trzeba będzie to rozplątywać.

**Kryterium, które da się sprawdzić `cargo test` już dziś:** model wyjść musi pozwalać utworzyć
wyjście bez żadnego odpowiednika sprzętowego i przeprowadzić na nim pełne kafelkowanie.
**Kategoria:** tanie teraz, drogie później — ta sama co D-026 i D-011.
**Odniesienie:** D-011, D-016, D-025, D-026

## D-036 — Raspberry Pi jako klient zadań specjalnych
**Status:** ✅ **PRZYJĘTA** (2026-08-01) — nie dodaje etapu ani platformy, zaostrza wymagania

**Kontekst:** Zastosowanie sformułowane przez użytkownika: RPi jako klient do RDP, VPN, przeglądania
internetu, poczty, filmu i muzyki. Ważne, czym to **nie** jest: to nie jest nowa platforma (RPi jest
celem od D-002 i D-020) ani nowe funkcje do napisania — RDP, przeglądarka, klient poczty
i odtwarzacz to **Warstwa 2 specyfikacji, czyli obce programy do uruchomienia**. Wartością tego
scenariusza jest to, że **wskazuje konkretne braki w zakresie protokołów**.

**Decyzja 1 — rozszerzenie D-012 o cztery protokoły, bez których scenariusz się nie domyka:**

| Protokół | Po co | Bez niego |
|---|---|---|
| `linux-dmabuf-v1` | klatka z dekodera sprzętowego trafia do kompozytora bez kopiowania przez CPU | wideo na SBC jest **nieużywalne**, nie „wolne" |
| `wp_viewporter` | skalowanie powierzchni wideo bez skalowania w kliencie | odtwarzacz skaluje na CPU |
| `presentation-time` | synchronizacja obrazu z dźwiękiem | rozjazd A/V przy dłuższym materiale |
| `idle-inhibit-v1` | ekran nie gaśnie w trakcie filmu | wygaszacz w 12. minucie |

Pierwszy z nich jest warunkiem koniecznym, nie optymalizacją.

**Decyzja 2 — sprzęt.** Realnym klientem jest **RPi 4 (4 GB) lub 5**. RPi 3 (1 GB, VideoCore IV)
nie udźwignie przeglądarki z wideo i **zostaje tym, czym był: najostrzejszym progiem testowym
dla lekkości samego shella** (D-002, D-027) — w tej roli jest cenniejszy niż jako klient.
Uwaga do scenariusza „podłączam do peceta kablem": tryb USB gadget mają RPi Zero, 4 i 5;
**RPi 3 go nie ma**, tam zostaje sieć.

**Decyzja 3 — wysyłanie obrazu na telewizor.** Rozdzielić dwie rzeczy, które w potocznym języku
są jedną:
- **„wyślij film na TV"** (DLNA/UPnP, Chromecast) — działa, istnieją gotowe narzędzia, sensowne;
- **„duplikuj ekran na TV"** (Miracast) — **poza zakresem**. `miraclecast` jest półporzucony,
  wymaga Wi-Fi P2P w sterowniku i jednoczesnej pracy P2P z normalnym Wi-Fi, czego tanie układy
  (w tym w RPi) często nie robią stabilnie. Wraca do rozmowy dopiero, gdy ktoś wykaże, że konkretny
  sprzęt to potrafi.

Kolejność: **HDMI → DLNA → Miracast (poza zakresem)**. HDMI nie jest tu kompromisem — to dokładnie
ten sam mechanizm co drugi monitor z D-026, więc mamy go w planie od M1 i jest jedyną w pełni
niezawodną drogą.
**Odniesienie:** D-002, D-012, D-020, D-026, D-027, D-035

## D-037 — Adapter systemowy: pakiety i usługi za wspólnym interfejsem
**Status:** ✅ **PRZYJĘTA** (2026-08-01) — jedyna z decyzji z 2026-08-01, która dotyka kodu wprost

**Kontekst:** Specyfikacja przewiduje własny panel sterowania i menedżer usług. Nawet po D-034
(Debian na PC) telefon prawdopodobnie pojedzie na postmarketOS, czyli Alpine z OpenRC zamiast
systemd i `apk` zamiast `apt`. Dwa środowiska są więc pewne, nie hipotetyczne.

**Decyzja:** Operacje systemowe idą przez **osobny crate za traitem** (roboczo `gostui-system`),
z implementacjami per środowisko. Panel sterowania i menedżer usług rozmawiają wyłącznie z tym
interfejsem i **nie wiedzą, na jakiej dystrybucji stoją**. Zakres jest mały i policzalny:

```
pakiety:  zainstaluj · odinstaluj · szukaj · lista_aktualizacji
usługi:   lista · uruchom · zatrzymaj · włącz_przy_starcie · wyłącz_przy_starcie
```

Implementacje: `apt + systemd` (Debian, PC i RPi) oraz `apk + OpenRC` (Alpine, telefon).

**Czego zakazuje:** wywołania `systemctl` ani `apt` na sztywno w kodzie UI. To jest ta sama granica
co D-016, tylko na innej osi: tam kod nie wie, że pod spodem jest smithay — tu nie wie, że pod
spodem jest Debian.

**Poza zakresem adaptera**, bo już wystandaryzowane przez D-Bus i różnic praktycznie nie ma: sieć
(NetworkManager), uprawnienia administracyjne (polkit, D-013), powiadomienia, portale.

**Czego ta decyzja nie oznacza:** nie piszemy własnego menedżera pakietów. Sam program to weekend
pracy; wartością jest repozytorium — dziesiątki tysięcy pakietów budowanych, testowanych,
podpisywanych i przebudowywanych przy każdym CVE. Menedżer pakietów bez repozytorium to sklep
bez dostawców. Patrz D-034.
**Odniesienie:** D-013, D-016, D-024, D-034

---

## D-038 — Pamięć mierzona jako prywatna, nie jako RSS
**Status:** ✅ **PRZYJĘTA** (2026-08-01) — **zastępuje progi z D-029**, reszta D-029 bez zmian.
D-029 przechodzi w status **ZASTĄPIONA w części dotyczącej progów**; jej uzasadnienie („dwa progi,
bo sterownik nie jest naszym kodem") zostaje w mocy i jest tu doprowadzone do końca.

**Problem, który to wywołał.** Po kroku 3 M2 pomiar dał 97 MB (GLES2) i 101 MB (Pixman) — czyli
„próg 80 MB przekroczony". Rozbicie procesu na mapowania pokazało, że ten wniosek był fałszywy:

| Co | RSS | z tego prywatne |
|---|---|---|
| `libLLVM.so` (kompilator shaderów Mesy, wciągany przez `radeonsi`) | 46,6 MB | 6,6 MB |
| `libgallium.so` | 13,0 MB | 1,2 MB |
| **nasza sterta + pamięć anonimowa** | **22,9 MB** | **22,9 MB** |
| binarka `gostui` | 5,8 MB | 0 |
| pula `wl_shm` klienta | 3,8 MB | 0 |

**Dwie trzecie RSS to mapowania plikowe Mesy**, współdzielone z każdą inną aplikacją GL w sesji —
a na docelowej ścieżce starego peceta (DRM + Pixman, kanwa prosto do dumb buffera, **bez EGL**)
nie będzie ich wcale. Mierzenie RSS w backendzie zagnieżdżonym daje więc fałszywy alarm dziś
i fałszywy spokój jutro.

**Decyzja — progi na pamięci prywatnej brudnej** (`Private_Dirty` z `/proc/<pid>/smaps`), czyli
tej, której nikt z nami nie dzieli i która znika razem z procesem:

| Ścieżka | Próg pamięci prywatnej | Zmierzone 2026-08-01 |
|---|---|---|
| Pixman (CPU) | **≤ 50 MB** | 31,9 MB |
| GLES2 | **≤ 70 MB** | 27,5 MB |

RSS zostaje jako liczba **raportowana, nie egzekwowana** — jest przydatna do porównań z innymi
środowiskami (sesja XFCE na tej stacji: 340,7 MB w sześciu procesach), ale nie nadaje się
na kryterium, bo zależy od sterownika bardziej niż od nas.

**Warunek ważności pomiaru:** liczba z backendu `winit` jest **orientacyjna**, bo nawet ścieżka
CPU trzyma tam kontekst EGL. Pomiar rozstrzygający robi się na DRM (M4) albo pod
`LIBGL_ALWAYS_SOFTWARE=1`. Dla porządku, zmierzone pod `llvmpipe`: 52,0 MB (GLES2)
i 61,4 MB (Pixman) prywatnych — czyli **oba progi przekroczone przy renderowaniu programowym
przez Mesę**, co jest dokładnie tym scenariuszem, którego DRM + Pixman ma unikać.

**Prognoza na stan końcowy** (szacunek, nie pomiar): ikony +5–10 MB, więcej tekstu +3–5 MB,
bufory scanout przy 1080p +17 MB, kanwa +4 MB, pule klientów +6–8 MB, reszta protokołu +5 MB.
Daje to **60–85 MB RSS na starym pececie bez EGL** i mieści się w progach powyżej — **pod
warunkiem D-039.**

**Stan wdrożenia:** testu nadal nie ma, tak jak przy D-029. Zmieniło się to, że wiadomo, **co**
ma mierzyć, a to była właściwa przeszkoda.

---

## D-039 — Żaden cache w tym procesie nie rośnie bez ograniczenia
**Status:** ✅ **PRZYJĘTA** (2026-08-01) — wywołana zmierzonym wyciekiem, nie przeczuciem

**Problem.** Cache tekstu w `gostui-render` jest kluczowany **całym napisem**, a `clear_cache`
nie było wołane z żadnego miejsca produkcyjnego. Zegar tworzy nowy napis co minutę. Zmierzone:
**1440 wpisów i 5,3 MB na dobę pracy**, czyli ~160 MB na miesiąc — w powłoce, która na maszynie
z 2 GB ma chodzić tygodniami i której cały budżet to 50 MB.

To nie jest błąd cache'u tekstu. To wzorzec, który powtórzy się wszędzie tam, gdzie klucz zależy
od danych zmieniających się w czasie: **tytuły okien** na dolnym pasku (klient zmienia tytuł, ile
chce), ikony per rozmiar, miniatury kart, wyniki wyszukiwania w Menu Start.

**Decyzja:** każdy cache w procesie kompozytora ma **jawny limit** i **test, który go pilnuje**.
Test ma kształt: „N różnych kluczy zostawia ≤ M wpisów", gdzie N jest wyraźnie większe od M.
Cache bez limitu jest w tym projekcie traktowany jak wyciek pamięci, bo nim jest — proces żyje
tygodniami, a nie sekundy jak w narzędziu wsadowym.

**Wyrzucanie:** najdawniej używany wpis, licznik użyć aktualizowany przy trafieniu. Nie LRU
z listą — przy limitach rzędu setek wpisów liniowe szukanie minimum jest tańsze niż utrzymywanie
struktury, a na starym procesorze prostota kodu jest wartością samą w sobie (D-027).

**Czego ta decyzja nie mówi:** że cache jest zły. Zegar bez cache'u rasteryzowałby ten sam napis
co minutę bez potrzeby. Rzecz w tym, że cache jest **optymalizacją z budżetem**, a nie miejscem,
gdzie odkłada się wszystko, co kiedykolwiek narysowaliśmy.

---

## D-040 — Procesy poza powłoką startują na żądanie
**Status:** ✅ **PRZYJĘTA** (2026-08-01) — konsekwencja budżetu z D-038, dotyczy M5, M7 i M8

**Kontekst.** Budżet z D-038 dotyczy jednego procesu, a użytkownik widzi sumę sesji. Punkt
odniesienia zmierzony na stacji: XFCE to 340,7 MB w sześciu stale działających procesach
(`xfwm4` 116,2 · `xfdesktop` 69,8 · `xfce4-panel` 48,5 · `xfsettingsd` 50,3 · `xfce4-session` 27,8
· `Thunar` 28,1). Większość z nich nic nie robi przez większość czasu.

**Decyzja:** poza kompozytorem **nic nie działa stale**.

| Składnik | Kiedy startuje | Szacowany koszt |
|---|---|---|
| XWayland (M5) | przy **pierwszym** kliencie X11, nigdy z góry | 40–60 MB |
| menedżer plików (M6) | gdy użytkownik go otworzy | 25–40 MB |
| panel sterowania, menedżer usług (M7, M8) | gdy użytkownik je otworzy | ~25 MB każdy |

Daje to sesję ~70 MB w spoczynku i ~150–170 MB przy otwartym menedżerze plików i aplikacji X11 —
**mniej więcej połowa XFCE**, co jest sensem D-027: na maszynie z 2 GB przeglądarce zostaje
~1,8 GB.

**Konsekwencja dla kodu, nie tylko dla planu:** panel sterowania i menedżer usług nie mogą trzymać
stanu, którego powłoka potrzebuje do działania. Jeśli powłoka zacznie pytać panel o cokolwiek,
panel będzie musiał działać zawsze — i decyzja przestanie obowiązywać sama z siebie.

---

## D-041 — Wejście: powłoka bierze jeden modyfikator, klawisze są liczbami w core
**Status:** ✅ **PRZYJĘTA** (2026-08-02) — realizowana w kroku 4 M2

**Kontekst.** Krok 4 M2 wymagał rozstrzygnięcia trzech rzeczy naraz: **co** powłoka przechwytuje
z klawiatury, **jak** wejście jest reprezentowane po stronie core (który nie może zależeć
od bibliotek systemowych), i **kiedy** zmienia się fokus. Wszystkie trzy są nieodwracalne
w tym sensie, że zmiana każdej z nich później oznacza przepisanie routingu, a nie dopisanie.

**Decyzja — trzy części:**

1. **Powłoka posiada `Super` i nic więcej.** Każdy skrót powłoki niesie `Super`; `Alt+Tab`,
   gołe klawisze funkcyjne i wszystko pozostałe należy do aplikacji. Modyfikatory dopasowują się
   **dokładnie**: `Ctrl+Super+Tab` to nie `Super+Tab`, bo dopasowanie „z nadmiarem" oznacza
   zjadanie kombinacji, których nam nie powierzono, w sposób niewidoczny dla aplikacji.
   Test tego jest w `gostui-core::input` i sprawdza **regułę**, nie bieżącą listę skrótów.

2. **Klawisze w core są liczbami w numeracji xkb, bez zależności od `libxkbcommon`.**
   Kompozytor musi mieć xkb — to on zamienia scancode na symbol w układzie użytkownika
   (polski układ ma pozostać polskim). Core nie może: crate ciągnący bibliotekę C przestaje się
   budować na kolejnej platformie. Granicą jest `Keysym(u32)`, a numeracja jest **pożyczona**
   z X11, żeby tłumaczenie po stronie kompozytora było `Keysym(raw.raw())`, a nie tablicą,
   której nikt nie utrzyma. **Pożyczenie numeracji to nie to samo, co zależność od biblioteki.**

3. **Fokus zmienia klik, nie najechanie.** Okno nie może zmienić się pod kursorem, który tylko
   przez nie przejeżdża, a na ekranie dotykowym „najechanie" nie istnieje w ogóle — reguła
   z D-020 („żadna funkcja wyłącznie po najechaniu") dotyczy także fokusu.
   Fokus podąża za kafelkiem: klawiatura trafia **wyłącznie** do okna, które trzyma kafelek;
   okno zepchnięte na dolny pasek fokus traci, bo pisanie do czegoś niewidocznego jest gorsze
   niż pisanie donikąd.

**Co z tego wynika w kodzie.** Trafienie punktu w strefę (`hit_test`) i tablica skrótów (`Keymap`)
siedzą w `gostui-core::input` i mają testy bez kompozytora (D-016). Geometria chipów dolnego paska
przeniosła się przy tej okazji do `gostui-core::shell`, bo **klik musi trafiać w chip, który
został narysowany** — dwie kopie tej arytmetyki to dwie odpowiedzi czekające, aż się rozjadą.

**Ruch wskaźnika nie rysuje klatki.** Obraz powłoki nie zależy od pozycji kursora, więc
przejechanie myszą przez okno nie przerysowuje niczego; klient dostaje swoje zdarzenia i tyle.
`request_redraw` na ścieżce ruchu to kilkaset klatek na sekundę i cicha likwidacja wymagania
z D-027. Dlatego doszedł osobny powód klatki (`input`) — żeby to było widać w pomiarze, a nie
w domysłach.

**Znane ograniczenia, świadome:**
- **`Super+Tab` nie działa w trybie zagnieżdżonym**, bo `xfwm4` trzyma go dla `switch_window_key`
  (tak samo `Super`+strzałki). Klawisz nie dociera do naszego okna — to własność sesji-gospodarza,
  nie usterka. Na gołym metalu (M4) znika. Zmierzone 2026-08-02.
- **Kursor nie jest rysowany.** W oknie zagnieżdżonym rysuje go sesja XFCE, a drugi byłby drugim
  kursorem. Własny kursor wchodzi z M4 (na tty nie ma kto go narysować) i jest warunkiem D-022.
- **Ograniczenie wskaźnika: blokada tak, zamknięcie w regionie nie.** `lock` jest aktywowany
  (kursor stoi, klient dostaje ruch względny — tego chce i gra, i wirtualny gładzik z D-022);
  `confine` nie jest aktywowany, bo przycinania do regionu nie ma, a ograniczenie przyznane
  i nieegzekwowane jest gorsze niż nieprzyznane.
- **Dotyk jest osobną ścieżką, ale nieprzetestowaną na sprzęcie** — stacja nie ma ekranu
  dotykowego, a backend zagnieżdżony na X11 nie wytworzy zdarzenia dotyku. Ścieżka istnieje,
  bo dorobienie jej później to przepisanie routingu (D-020, D-022), nie jego rozszerzenie.

**Odniesienie:** D-016, D-020, D-022, D-025, D-027

---

## D-042 — Pełny ekran zakrywa paski, a wyjście z niego należy do powłoki
**Status:** ✅ **PRZYJĘTA** (2026-08-02) — realizowana w kroku 5 M2

**Kontekst.** Reguła z D-025 mówi, że okno nigdy nie zakrywa pasków, bo dolny pasek jest jedyną
drogą do pozostałych okien. Pełny ekran jest jedynym przypadkiem, w którym ta reguła szkodzi:
film z paskiem w poprzek nie jest pełnym ekranem, a gra dostaje obcięty obraz. Każda aplikacja
rozumie przez „pełny ekran" to samo i zgłosi naszą interpretację jako błąd.

**Decyzja:** okno pełnoekranowe dostaje **całe wyjście, razem z oboma paskami**, i jest jedyną
powierzchnią rysowaną nad nimi. W liście wyświetlania jest to **pozycja na końcu listy**, a nie
drugi stan („paski ukryte") do utrzymywania w prawdzie — wyjątek jest jednym miejscem w jednej
liście i ma test.

**Warunek, bez którego decyzja nie obowiązuje:** wyjście z pełnego ekranu jest **skrótem powłoki**
(`Super+F`), nie żądaniem do klienta. Okno zakrywające oba paski nie może uwięzić użytkownika,
gdy aplikacja przestanie odpowiadać — a przy dotyku bez klawiatury nie ma alternatywnej drogi
ucieczki (D-020). Powłoka przełącza swój własny stan, klient dostaje `configure` i flagę
`Fullscreen`, i może się do niej dostosować — ale nie ma głosu.

**Odniesienie:** D-020, D-025, D-041

---

## D-043 — Kafelki nie mają pasków tytułu; tożsamość okna żyje na dolnym pasku
**Status:** ✅ **PRZYJĘTA** (2026-08-02) — realizowana w kroku 5 M2

**Kontekst.** Kompozytor ogłasza `xdg-decoration` i mówi każdemu klientowi `ServerSide` (D-025:
kafelek nie rysuje własnej ramki), co znaczy, że dekoracja jest **naszą** decyzją projektową.
Pytanie brzmiało, co narysować.

**Decyzja:** **nic poza ramką fokusu.** Żadnego paska tytułu, żadnego przycisku zamknięcia
na oknie.

**Dlaczego to nie jest oszczędzanie na robocie:**
- Pasek tytułu służy głównie do **chwytania okna**, a okien się tu nie przesuwa i nie skaluje
  krawędzią (D-025). Zostałby paskiem do patrzenia, kosztem wysokości każdego kafelka —
  na telefonie kosztem znaczącym.
- To, do czego pasek służy poza chwytaniem — nazwa okna i droga powrotna do niego — **już
  istnieje na dolnym pasku**, który jest celem dotykowym (≥ 48 px) i nie powiela się per okno.
- Zamykanie ma `Super+Q` i zamknięcie z poziomu aplikacji; osobny „krzyżyk" na każdym kafelku
  to trzeci sposób na to samo i najmniejszy cel na ekranie.

**Ramka rysowana jest wewnątrz prostokąta okna, nie w odstępie między kafelkami.** Odstęp jest
ustawieniem motywu i może wynosić zero, a ramka fokusu znikająca przy `inner_gap = 0` jest ramką,
na której nie można polegać. Kosztem są dwa zewnętrzne piksele obrazu klienta. Okno pełnoekranowe
ramki nie dostaje (D-042).

**Czego to nie naprawia:** **GTK nie implementuje `xdg-decoration`** i rysuje własny nagłówek
niezależnie od tego, co powiemy. Sprawdzone 2026-08-02: `GTK_CSD=0` niczego nie zmienia.
Qt i klienci honorujący protokół (np. `foot`) wyglądają tak, jak zaprojektowano. Polityka
per aplikacja — jeśli w ogóle — jest tematem na M3, nie kłótnią z toolkitem.

**Odniesienie:** D-020, D-025, D-032, D-042

---

## D-044 — Kierunek wizualny: Final Cartridge III i stary Windows, nie współczesny desktop
**Status:** ✅ **PRZYJĘTA** (2026-08-02) — potwierdzenie użytkownika po zobaczeniu działającej powłoki

**Kontekst.** Do tej pory wygląd wynikał z zakazów: bez animacji dekoracyjnych (D-021), bez efektów,
kolory i rozmiary jako dane (D-032). Zakazy mówią, czego **nie** robić, i nie wystarczają, gdy
w M3 trzeba narysować slider kart, Menu Start i kafle. Po zobaczeniu powłoki z dwoma klientami
użytkownik nazwał kierunek wprost i to jest materiał na decyzję, a nie na komplement.

**Punkty odniesienia, podane przez użytkownika:** **Final Cartridge III (C64)** i **stary Windows**.
Wspólne im jest to samo, i to jest treść tej decyzji:

- **płaskie prostokąty i wyraźne krawędzie** zamiast cieni, gradientów i zaokrągleń;
- **granica między strefami widoczna gołym okiem** — jedna kreska, nie subtelny odcień;
- **element jest albo aktywny, albo nie**, i widać to z drugiego końca pokoju (stąd ramka fokusu
  w kolorze akcentu, D-043, a nie delikatne przyciemnienie);
- **nic nie rusza się bez powodu** — statyczny obraz jest stanem, nie klatką animacji;
- **gęstość informacji ponad przestronność**: puste miejsce ma być miejscem na treść, nie
  oddechem między elementami.

**Dlaczego to jest decyzja techniczna, nie gust.** Ten język wizualny jest **zgodny z resztą
projektu, a nie obok niej**: płaskie prostokąty to dosłownie to, co potrafi lista wyświetlania
(`Fill` + `Text` + `Surface`), więc ścieżka Pixman i GLES2 rysują je identycznie (D-005, D-027),
a rysowanie kosztuje tyle, ile trwa zapełnienie pikseli. Cienie i zaokrąglenia wymagałyby
mieszania per piksel na obu ścieżkach — czyli dokładnie tego, czego na starym PC i telefonie
nie chcemy płacić. **Estetyka wybrana tu zgadza się z budżetem wydajności**, i to jest powód,
dla którego zostaje zapisana, a nie tylko zapamiętana.

**Konsekwencja dla M3:** slider kart, Menu Start i kafle żywe (D-033) projektujemy w tym języku.
Jeśli któryś element wymaga cienia albo animacji, żeby był czytelny, to znaczy, że jest źle
rozłożony — nie że potrzebuje efektu.

**Odniesienie:** D-021, D-031, D-032, D-033, D-043, `gostos.md` §B

---

## D-045 — Klient-fuzzer mieszka w osobnym crate i mówi protokołem obiema stronami
**Status:** ✅ **PRZYJĘTA** (2026-08-02) — wdrożona w `crates/gostui-fuzz-client`

**Kontekst.** Zasada odporności z `docs/04-zasady-pracy.md` mówi, że błąd protokołu ma zabić
klienta, a nie kompozytor, i kryterium M2 wymaga tego wprost. Do 2026-08-02 nic tego nie
sprawdzało — a zasada, której nikt nie egzekwuje, jest życzeniem. Pierwszy pełny przebieg
narzędzia znalazł **panikę osiągalną czterema słowami na gnieździe** (patrz niżej), więc
pytanie „czy warto" jest już odpowiedziane pomiarem.

**Decyzja.** Fuzzer jest **osobnym crate'em** `gostui-fuzz-client`, nie kolejną binarką
w `gostui-compositor`, i wolno mu zależeć od `wayland-client`.

**Dlaczego to nie łamie D-016.** D-016 trzyma `wayland-*` **z dala od logiki** — model kart,
kafelkowanie, mapa skrótów mają być testowalne bez kompozytora. Fuzzer nie jest logiką: mówienie
protokołem jest całym jego zadaniem, a testowalny jest tylko z działającym kompozytorem
z definicji. Osobny crate, bo żaden pojedynczy crate nie powinien trzymać **obu stron** tego
samego protokołu — `gostui-compositor` ma już `wayland-server`. Zależność nie jest nowa:
`wayland-client` 0.31 był w `Cargo.lock` przez `winit`, więc `cargo deny` dostaje krawędź
w grafie, a nie pakiet do przejrzenia.

**Dwie rzeczy konstrukcyjne, które wynikły z pomiaru, nie z projektu:**

1. **Klient sam otwiera gniazdo i klonuje je** przed oddaniem `Connection`. Dzięki temu jeden
   klient robi poprawne uzgodnienie przez bibliotekę (globale, pool `wl_shm` z prawdziwym
   deskryptorem) i wysyła surowe śmieci na to samo połączenie. Połowa ataków jest
   **niewyrażalna** przez typowane API — żądanie do nieistniejącego obiektu, opcode spoza
   interfejsu, nagłówek kłamiący o długości — a fuzzer atakujący z pustego stanu nie dosięga
   niczego ciekawego.
2. **Odczyt odpowiedzi nigdy nie blokuje.** Kompozytor, który dostał zapowiedź 64 bajtów
   i 8 bajtów treści, **słusznie** zatrzymuje fragment i obsługuje dalej innych klientów
   (zmierzone: `wayland-info` bindował wszystkie globale w trakcie scenariusza wiszącego cztery
   minuty). Czekanie na odpowiedź, której poprawne zachowanie nigdy nie przyśle, zawiesza
   **fuzzera na kompozytorze zdającym egzamin** — tak wyglądał pierwszy przebieg. Biblioteka
   czeka w `poll()`, którego nie skraca żadna flaga gniazda, więc odpowiedź czytana jest surowo
   z timeoutem, a „brak odpowiedzi" jest osobnym, poprawnym wynikiem.

**Kryterium zaliczenia scenariusza nie jest przeżycie klienta** — większość ma zostać wyrzucona.
Zalicza się wtedy, gdy zaraz po nim **świeże, poprawne połączenie** wykonuje pełny roundtrip po
`wl_registry`. To odróżnia kompozytor, który się obronił, od takiego, który po cichu umarł
trzymając gniazdo otwarte. Weryfikacja jest w binarce, nie w skrypcie obok, żeby kod wyjścia
znaczył coś sam z siebie.

**Nie jest testem `cargo test`** — wymaga działającego kompozytora, którego CI nie ma. CI go
kompiluje, co dzieje się samo przez `--workspace`.

**Co znalazł pierwszy przebieg (2026-08-02):** `xdg_surface.set_window_geometry(0, 0, -1, -1)`
→ `Size::new` w smithayu 0.7.0 **panikuje** → cały kompozytor pada, a z nim wszystkie aplikacje
użytkownika. Usterka jest w zależności, ale czekanie na poprawkę z góry nie wchodzi w grę:
profil `release` ma `panic = "abort"`, więc żaden `catch_unwind` nie stanie między żądaniem
klienta a procesem. Stąd xdg-shell jest delegowany **interfejs po interfejsie** zamiast przez
`delegate_xdg_shell!`, a `xdg_surface` dostaje jedno sprawdzenie przed delegacją.
Odrzucamy wyłącznie wartości ujemne — zero też jest wbrew protokołowi, ale smithay je przeżywa,
a rozłączanie klienta za wartość, która zawsze działała, to regresja kupiona cudzym oknem.

**Odniesienie:** D-016, D-027 (`panic = "abort"`), `docs/01-strategia-dev-test.md` §4 M2 krok 6,
`docs/04-zasady-pracy.md` § Odporność

## D-046 — Karta jest kolumną o stałej szerokości, liczba kart wynika z ekranu
**Status:** ✅ **PRZYJĘTA** (2026-08-03) — **zastępuje D-031**, wdrożona
**Kontekst:** Środkowa strefa stała, bo D-031 była odstępstwem od `gostos.md` §B i wymagała
rozmowy. Rozmowa wykazała, że **spór był źle postawiony**: i §B (jedna karta ze skrawkami
sąsiadów), i D-031 (pasek nazw + tablica kafli na całą szerokość) milcząco zakładały **jedną
kartę widoczną naraz**. To jest model telefonu. Użytkownik: „jedna karta na środku to model
telefonu. Na pulpicie tych kart naturalnie musi być więcej, w zależności od rozdzielczości.
Na moim monitorze spokojnie zmieściłoby się 7. Chciałbym, żeby karty były pogrupowane i żeby
reprezentowały konkretny obszar."

**Decyzja:** karta to **kolumna o stałej szerokości** (`Metrics::card_width`, domyślnie 260)
na pełną wysokość strefy aplikacji. **Liczba widocznych kart nie jest ustawieniem** — wynika
z szerokości wyjścia. Karta wygląda tak samo na monitorze i na telefonie; zmienia się tylko
to, ile ich widać.

| Wyjście | Strefa aplikacji | Kart widocznych |
|---|---|---|
| monitor 1920×1080 | 1920×984 | **7** |
| telefon poziomo 780×360 | 780×264 | 2 pełne + skrawek trzeciej |
| telefon pionowo 360×780 | 360×684 | 1 pełna + skrawek |

**To rozwiązuje konflikt zamiast go rozstrzygać — i to jest cała treść tej decyzji:**

1. **Skrawek przestaje być funkcją do zaprojektowania.** Karta, która się nie mieści, jest
   rysowana przycięta do krawędzi strefy. To *jest* „skrawek jako wizualna podpowiedź, że jest
   kolejna karta" z §B. **Odstępstwo od specyfikacji znika — `gostos.md` nie wymaga zmiany.**

   > **Sprostowanie (2026-08-05):** to zdanie było prawdziwe dopiero od D-047. Do tej daty
   > `card_columns` **zwężał prostokąt** karty wychodzącej poza strefę, a `layout_tiles`
   > odpowiadał poprawnie na pudełko, które dostał: za wąskie na jedną kolumnę kafli, więc
   > zero kafli. Skrawek nie był kartą przyciętą, tylko **przeliczoną od nowa** — rysował się
   > pusty i nie podpowiadał niczego. Argument, którym ta decyzja zamknęła D-031, obronił się
   > więc dopiero po naprawie, nie w dniu zapisania.
2. **Konflikt gestów z D-030 znika.** Istniał wyłącznie przy tablicy kafli na całą szerokość,
   przewijanej w bok. Przy kolumnach osie są rozłączne: **w bok = przewijanie kart, w pionie =
   przewijanie kafli w karcie.**
3. **Trzy rzeczy ze specyfikacji odzyskują miejsce**, które D-031 im zabierała: ikony funkcyjne
   karty (nagłówek kolumny, `Metrics::card_header`), resize przeciąganiem (krawędź kolumny),
   karta przypięta z D-009 (pierwsza z lewej, nie przewija się).

**Przewinięcie jest wyliczane, nie przechowywane.** `CardLayout::first` wynika z aktywnej karty,
bo przewijanie ma dokładnie jedną przyczynę: karta z fokusem musi być widoczna. Osobne pole
byłoby drugą odpowiedzią na pytanie, które ma już jedną, a `Super+←/→` (D-007) musiałoby
pamiętać o jej aktualizacji.

**Liczba kolumn kafli w karcie też nie jest ustawieniem** — wynika z miejsca po odjęciu
marginesów, tak jak `tile_limit` i `bottom_bar_layout` już liczą swoje. Przy domyślnych
metrykach wychodzą dwie, ale **nigdzie w kodzie nie jest napisane „dwie"**: zwężenie karty
w motywie daje jedną kolumnę zamiast sprzeczności między dwiema liczbami, które obie miały
być prawdziwe.

**Wdrożenie (2026-08-03):** `gostui_core::shell::card_columns` i `layout_tiles` z 8 testami;
`hit_test` zwraca `Hit::Card` i `Hit::CardTile`, czytając **te same dwie funkcje** co painter —
`paint.rs` nie liczy już żadnej pozycji sam, co domyka regułę „layout jest logiką, nie
rysowaniem", którą stary `slider()` łamał. Złote obrazy przeliczone; doszła piąta scena
`monitor-przewiniety` (9 kart), żeby skrawek i przewinięcie były pilnowane pikselem.

**Naprawione po pierwszym uruchomieniu (2026-08-03), bo to jest treść, nie szczegół:**
**każdy punkt karty należy do tej karty, łącznie z kaflami.** Pierwsza wersja traktowała
kafel jako należący do niczego — z rozumowania, że kafel będzie kiedyś uruchamiał program
i nie powinien „po cichu" przełączać karty. Zmierzone na działającej powłoce: przy oknie
1360×850 karta ma 260×754, a jej kafle zajmują 204 szerokości i 312 wysokości, **czyli około
jednej trzeciej kolumny, u góry, gdzie idzie oko i za nim palec**. Efekt: klikanie karty
działało „czasem tak, czasem nie", zależnie od tego, ile skrótów miała karta. Uruchamianie
i aktywacja nigdy nie były w konflikcie — `hit_test` nadal odróżnia kafel od karty (bo
uruchamianie musi wiedzieć który), a `Hit::card` odpowiada na osobne pytanie „czyja to karta".
Reguła siedzi w core i ma test chodzący po każdym punkcie każdej kolumny.

**Odniesienie:** `gostos.md` §B (spełnione, bez odstępstwa), D-007, D-008, D-009, D-016, D-030,
D-031 (zastąpiona), D-032, D-044, D-047

---

## D-047 — Pasek kart wyśrodkowany na aktywnej karcie, z zaciskiem na końcach
**Status:** ✅ **PRZYJĘTA** (2026-08-05) — **uzupełnia D-046**, wdrożona

**Kontekst:** D-046 postawiła kolumny, ale nie powiedziała, **gdzie stoi pasek**. Odpowiedź
domyślna — pasek przy lewej krawędzi, przewijany dopiero wtedy, gdy aktywna karta wypadłaby
poza ekran — dawała dwa widoczne skutki. Na monitorze cztery karty zajmowały 1076 z 1920
jednostek i **844 zostawało jako czarna dziura po prawej**. Na telefonie pionowo, gdzie mieści
się jedna karta, sąsiad pokazywał się **tylko z prawej strony**, a `gostos.md` §B pisze
o skrawkach *po bokach*, w liczbie mnogiej. Użytkownik: „musimy jakoś środkować te karty (…)
środkowa karta to ta pierwsza aktywna, więc i na telefonie będzie widoczna".

**Decyzja — trzy reguły, jedna funkcja:**

1. **Aktywna karta stoi na środku strefy.** Przesunięcie paska liczone jest w jednostkach,
   nie w całych kartach, więc sąsiedzi wystają poza ekran obiema stronami i widać ich skrawki.
2. **Na końcach pasek jest zaciśnięty**, nie pływa. Pierwsza karta nie wchodzi na środek,
   zostawiając pustkę po lewej: puste miejsce tam, gdzie oko spodziewa się karty, czyta się
   jako usterka, nie jako układ, a powłoka stawiająca gęstość nad przestronność (D-044) nie ma
   z niego pożytku. **Zacisk ratuje też regułę z D-007** — `Super+←` na pierwszej karcie ma nic
   nie przesuwać i nie rysować klatki; przy pasku pływającym ten skrót nadal zmieniałby offset,
   czyli przytrzymana strzałka znów byłaby pętlą renderującą.
3. **Pasek węższy niż strefa jest wyśrodkowany w całości.** Nadmiar wydany na jedną stronę jest
   dziurą; rozdzielony na dwie jest marginesem.

**Cena, świadomie przyjęta:** przy przewijaniu skrawek bywa **kilkujednostkowy** — pasek
przesuwa się płynnie, więc karta wchodzi w kadr od zera. Pytanie „czy wprowadzać minimalną
szerokość widocznego skrawka" pozostaje otwarte i **staje się przez tę decyzję częstsze**,
ale nie blokuje: włos na krawędzi jest uczciwym wynikiem przewijania ciągłego, nie usterką.

**Konsekwencja dla geometrii, ważniejsza niż samo wyśrodkowanie:** prostokąty kart są odtąd
**pełnej szerokości, także te wystające poza strefę**, a przycina je rasteryzator
(`fill_rect` i `blend_image` klipują z każdej strony, więc **do listy wyświetlania nie wchodzi
żadne nowe pojęcie** i obie ścieżki renderera dziedziczą to za darmo). To właśnie ta zmiana
naprawiła pusty skrawek opisany w sprostowaniu przy D-046 — i jest jedynym powodem, dla
którego kafle w karcie przyciętej leżą tam, gdzie w każdej innej.

**Odniesienie:** `gostos.md` §B, D-007, D-021 (przesuwanie palcem pójdzie tą samą arytmetyką —
offset w jednostkach jest tym, czego wymaga podążanie 1:1), D-044, D-046

---

## Stan rozstrzygnięć (2026-08-01)

**Wszystkie decyzje blokujące start są zamknięte.** M0 może ruszyć.

| | Decyzja | Rozstrzygnięcie |
|---|---|---|
| D-016 | Granica core ↔ kompozytor | Logika w crate'ach bez zależności od smithaya |
| D-001 | Renderer | GLES2 + Pixman ze smithaya, za wspólną abstrakcją |
| D-002 | RPi3 | Możliwy — rozwiązany przez D-001 |
| D-003 | Karty vs. okna | Model A — slider jako warstwa pulpitu, `Super+D` |
| D-014 | Nazwa i licencja | GOST OS (marka) / `gostui` (technicznie), GPL-3.0 |
| D-019 | Historia repo | Tag `v0-live-build-iso`, nowy `main` |
| D-020 | Platforma docelowa | Telefon jako cel, PC → RPi → telefon jako droga |
| D-021 | Animacje | Dekoracyjne zakazane, manipulacja bezpośrednia przy dotyku wymagana |
| D-022 | Tryb wskaźnika | Wirtualny gładzik dla obcych aplikacji, dotyk bezpośredni dla własnego UI |
| D-023 | Zakres portu | Bez aparatu, modemu i GPS; Wi-Fi i Bluetooth krytyczne |
| D-024 | System | Zapisywalny, normalna instalacja pakietów; żadnego immutable |
| D-025 | Model okien | Kafelkowanie z limitem widocznych okien; dialogi i popupy pływające |
| D-026 | Dobór telefonu | Wymagane wyjście obrazu (DP alt mode, SDM845); model wyjść jako kolekcja od M1 |
| D-027 | Stary PC | Główny cel wdrożeniowy; RSS < 80 MB jako test, ścieżka Pixman równorzędna |
| D-028 | smithay | 0.7.0 bez domyślnych cech, tylko `backend_winit`; Pixman zagnieżdżony jako CPU do tekstury — **warunek złotych obrazów spełniony 2026-08-02** |
| D-029 | Budżet RSS | **ZASTĄPIONA w części progów przez D-038**; uzasadnienie „dwa progi" w mocy |
| D-005 | Stack tekstowy | `cosmic-text`; tekst rasteryzowany raz, wykonywany przez obie ścieżki |
| D-032 | Motyw | Kolory, rozmiary i czcionki jako dane w `theme.toml`; podłoga dotykowa 48 |
| D-015 | Warsztat | Ubuntu na stacji, Debian minimalny w QEMU jako cel wdrożeniowy |
| D-034 | Podstawa systemu | **Debian** na PC i RPi; Arch odrzucony; własne = jeden `.deb` + repo + ISO |
| D-035 | Wyjście wirtualne | `Output` nigdy nie zakłada fizycznego ekranu; zdalny pulpit = headless + wstrzykiwane wejście |
| D-036 | RPi jako klient | Rozszerza D-012 o dmabuf/viewporter/presentation-time/idle-inhibit; RPi 4/5; Miracast poza zakresem |
| D-037 | Adapter systemowy | Pakiety i usługi za traitem; nigdy `systemctl` ani `apt` na sztywno w UI |
| D-038 | Budżet pamięci | Mierzymy **pamięć prywatną**, nie RSS: Pixman ≤ 50 MB, GLES2 ≤ 70 MB |
| D-039 | Cache | Każdy cache ma limit i test; cache bez limitu = wyciek (zmierzone 5,3 MB/dobę) |
| D-040 | Procesy sesji | Poza kompozytorem nic nie działa stale; XWayland dopiero przy kliencie X11 |
| D-041 | Wejście | Powłoka bierze wyłącznie `Super`; klawisze w core jako liczby bez xkb; fokus od kliku |
| D-042 | Pełny ekran | Zakrywa oba paski; wyjście `Super+F` należy do powłoki, nie do klienta |
| D-043 | Dekoracje | Bez pasków tytułu; sama ramka fokusu, tożsamość okna na dolnym pasku |
| D-044 | Kierunek wizualny | Final Cartridge III / stary Windows: płaskie prostokąty, ostre krawędzie, gęstość |

**Rekomendacje przyjmowane domyślnie** (bez sensownej alternatywy, do zakwestionowania w każdej chwili):
D-006 greeter poza Core, D-008 karta = siatka skrótów,
D-009 jedna karta przypięta, D-011 jednostki logiczne, D-013 zakres
rozszerzony o XWayland/schowek/powiadomienia/tray/portale/polkit,
D-018 a11y poza zakresem. (D-017 wyszło z tej listy — zaostrzone przez D-027.
**D-015 też z niej wyszło: domknięte 2026-08-01** przy okazji D-034.)

**Do sprawdzenia, zanim cokolwiek obiecamy:** wsparcie 32-bitowe (`i386`) — Debian trixie nie ma
już dla niego instalatora ani jądra, więc maszyny wyłącznie 32-bitowe są poza zakresem z powodu
dystrybucji, nie naszej decyzji. Patrz D-027.

**Nadal otwarte, ale nie blokujące:**
0. **D-030** (zapisana 2026-08-01) — orientacja pozioma telefonu. Propozycja do przeczytania
   **przed** kodem, który ją realizuje.
   **D-031 wyszła z tej listy: ZASTĄPIONA przez D-046** — spór o skrawki rozwiązał się sam,
   gdy karta stała się kolumną. **D-032 wyszła jako przyjęta i wdrożona**
   (`gostui-core::theme` + `theme.toml`). **D-033 przyjęta 2026-08-04 i wdrożona w części** —
   podpis kafla martwego jest, ikona i kafel żywy nie; kolejność w jej wpisie.
1. **D-004** (VFS vs. `sshfs`) — blokuje dopiero M6.
2. **D-012** (pełny zakres protokołów Wayland) — domykane stopniowo, per milestone.
   Ostatnie domknięcie: cztery protokoły wideo z D-036.
3. **Dystrybucja na telefonie** — **jedyna pozostałość po pytaniu o podstawę systemu**, bo strona
   PC-towa jest zamknięta przez D-034. postmarketOS (Alpine/`apk`, gotowa infrastruktura portów)
   vs. Mobian/Debian (`apt` jak na PC, ale port `rav` od zera). Rozstrzygnąć **przed** startem
   portu, nie wcześniej. GostUI nie zależy od wyboru — różnica dotyczy pakowania, nie kodu,
   a menedżer usług i panel pakietów są przed nią osłonięte adapterem z D-037.
   Patrz `03-cel-telefon.md` §8.1.
4. **Sposób przełączania trybu wskaźnika** (D-022) — przycisk w dolnym pasku czy gest? Do M3.
5. Waydroid jako uzupełnienie doboru aplikacji — mniej istotny niż wcześniej sądzono, skoro
   celem są normalne programy desktopowe (D-024), a nie aplikacje mobilne.

**Zrewidowane:**
- **D-010 (jedno wyjście w v1)** → **D-026**. Warunek rewizji spełniony: użytkownik szuka telefonu
  z wyjściem obrazu, więc scenariuszem docelowym jest stacja dokująca. Wiele wyjść nadal nie jest
  funkcją v1, ale model wyjść musi być kolekcją od M1, a gorące odłączenie wyjścia trafia
  do kryteriów akceptacji.

**Do rewizji, gdyby zmieniły się założenia:**
- **D-018 (a11y poza zakresem)** — patrz uwaga niżej.

**Uwaga do D-018 (a11y poza zakresem):** cel telefonowy (D-020) częściowo ją unieważnia —
duże cele dotykowe i pełna obsługa bez najechania to wymagania dotyku, które przy okazji są
podstawą dostępności. AT-SPI pozostaje poza zakresem, ale UI wychodzi z tego lepsze.
