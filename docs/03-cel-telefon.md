# Telefon jako cel główny — konsekwencje dla projektu

Ustalenie z 2026-07-30: docelową platformą jest **telefon (Motorola Moto G8 i dowolny inny)**,
przy czym **rozwój zaczyna się od PC i Raspberry Pi**. Ten dokument rozdziela dwie rzeczy, które
łatwo pomylić: co trzeba wbudować w shell **od początku**, a co może naprawdę poczekać.

---

## 1. Dobra wiadomość: ten projekt jest bardziej telefonowy niż komputerowy

Slider kart z przesuwaniem w bok to **natywny idiom telefonu**, nie komputera. Na desktopie jest
świeżą alternatywą dla pulpitu; na telefonie jest po prostu tym, jak działają ekrany domowe.
Podobnie: trzy strefy, minimalizm, brak zagnieżdżonych menu, jedna zbiorcza ikona statusu
rozwijana w panel — to wszystko są rozwiązania z telefonu.

**Nie trzeba przeprojektowywać UI.** Zmieniają się priorytety i kilka konkretnych decyzji,
opisanych w sekcji 3.

---

## 2. Zła wiadomość: Moto G8 nie ma portu i to osobny projekt

Fakty sprawdzone 2026-07-30:

| | |
|---|---|
| Moto G8, nazwa kodowa | **`rav`** |
| SoC | Snapdragon 665 = **SM6125** |
| GPU | **Adreno 610** |
| Port w postmarketOS (`pmaports`) | **nie istnieje** — `device-motorola-rav` daje 404 (podobnie `sofiar`, `doha`) |
| SM6125 w mainline Linux | ✅ **jest** — `arch/arm64/boot/dts/qcom/sm6125.dtsi` |
| Urządzenia SM6125 już w mainline | Sony Xperia 10 II (`pdx201`), Xiaomi `ginkgo`, `laurel-sprout`, `willow` |

Czyta się to tak: **platforma SoC jest zrobiona, konkretne urządzenie nie.** Port Moto G8 sprowadza
się do napisania device tree dla `rav` na bazie istniejącego `sm6125.dtsi` i dodania portu do
pmaports. To wykonalne, ale jest to **osobny projekt niż shell** — z własnymi ryzykami, które
nie mają nic wspólnego z GostUI: modem, kamera, uśpienie, ładowanie, czujniki.

**Wniosek strategiczny:** nie wiązać harmonogramu shella z portem telefonu. To dwie niezależne
prace, które spotykają się dopiero na końcu.

### 2.1 Konkretna rekomendacja sprzętowa

Jeśli chcesz weryfikować telefonowe założenia na prawdziwym sprzęcie, **nie zaczynaj od Moto G8**.

**Xiaomi Redmi Note 8T (`willow`)** — port istnieje w pmaports (gałąź `testing`), a to
**ten sam SoC SM6125 i ten sam GPU Adreno 610** co w Moto G8. Czyli identyczny stos sterowników:
`msm`/DRM, `freedreno`, ten sam sposób obsługi wyświetlacza i dotyku. Wszystko, co zadziała
na `willow`, zadziała na `rav` — a `willow` uruchamiasz dziś, bez pisania device tree.

Kolejność, która minimalizuje ryzyko:
1. Shell na PC (całość developmentu).
2. Weryfikacja dotyku — **tani ekran dotykowy USB podłączony do PC** albo oficjalny ekran do RPi.
   Sam dotyk da się przetestować bez telefonu.
3. Weryfikacja na telefonie z gotowym portem — potwierdza cały stos ARM64 + Adreno.
4. Dopiero potem port `rav` (Moto G8) — czyli device tree, już z działającym shellem w ręku.

#### Aktualizacja (2026-07-30, D-026): jeśli telefon ma dawać obraz na monitor

Powyższa rekomendacja `willow` zakładała jedno kryterium: „ten sam SoC co Moto G8".
Po dołożeniu **wymagania wyjścia obrazu** rekomendacja się zmienia — `willow`, jak cała rodzina
SM6125, wyjścia obrazu nie ma.

Obraz przez USB-C na telefonach z Androida to praktycznie wyłącznie **DisplayPort alt mode**,
a ten w świecie mainline/pmaports oznacza w praktyce **SDM845**. Sprawdzone, nie założone:

| Telefon | Nazwa kodowa | pmaports | Uwaga |
|---|---|---|---|
| **OnePlus 6** | `oneplus-enchilada` | **`community`** | najlepiej przetestowany, dużo dokumentacji |
| **OnePlus 6T** | `oneplus-fajita` | **`community`** | to samo, czytnik linii papilarnych pod ekranem |
| **Pocophone F1** | `xiaomi-beryllium` | **`community`** | dwa warianty panelu (`ebbg` / `tianma`) |
| SHIFT6mq | `shift-axolotl` | **`community`** | rzadki poza Niemcami |
| Sony Xperia XZ3 / 1 | `akari`, `akatsuki` | brak | DTS w mainline jest, portu w pmaports nie ma |

`community` to gałąź dojrzalsza niż `testing`, w której siedzi `willow` — czyli takie urządzenie
jest **łatwiejsze** niż Moto G8 pod każdym względem naraz: port istnieje, jest utrzymywany, ma
wyjście obrazu, a Adreno 630 jest mocniejsze od 610.

**Co to nie zmienia:** renderer zostaje GLES2 (D-001). SDM845 ma działający Turnip/Vulkan, ale
RPi3 i Adreno 610 nadal go nie mają, a jeden renderer jest tańszy w utrzymaniu niż dwa.

**Miejsce Moto G8 po tej zmianie:** sprzęt, który masz pod ręką — dobry do testowania dotyku,
skalowania i pionowej orientacji. Nie do scenariusza dokowania.

### 2.2 GPU — decyzja o rendererze była trafna

Adreno 610 obsługuje `freedreno`, który daje **OpenGL ES do 3.2**. Natomiast Turnip (Vulkan
dla Adreno) pełne wsparcie ma dopiero mniej więcej od Adreno 616 — dla 610 jest niepewne.

Czyli renderer **GLES2 ze smithaya (D-001) działa na telefonie, a ścieżka Vulkan/`wgpu` byłaby
tam ryzykowna.** Ta sama decyzja, która odblokowała RPi3, odblokowuje też Moto G8.

---

## 3. Co trzeba wbudować od razu, bo później jest drogie

To jest sedno dokumentu. Poniższe rzeczy kosztują **prawie nic teraz** i **bardzo dużo później**,
więc mimo że telefon jest krokiem trzecim, muszą wejść do M1–M3.

### 3.1 Dotyk jako pełnoprawne wejście **[do M2]**
`wl_touch` obok `wl_pointer`, od początku jako **osobna ścieżka**. Chodzi o to, żeby dotyk nie był
*wyłącznie* przemianowanym zdarzeniem myszy — bo wtedy przepadają gesty wielopalcowe, a interfejs
zakłada istnienie kursora i najechania, których przy dotyku bezpośrednim nie ma.

**To nie wyklucza trybu wskaźnika** — przeciwnie, tryb wskaźnika (§6) jest osobną, świadomie
włączaną ścieżką i wymaga dokładnie tego rozdzielenia, żeby dało się go czysto zaimplementować.

Konsekwencje dla UI, do wpisania w reguły layoutu:
- **Cele dotykowe ≥ 48 px logicznych.** Ikony funkcyjne na górze karty i przyciski w panelu
  `[SYSTEM]` muszą to spełniać — na desktopie i tak nie zaszkodzi.
- **Żadna funkcja nie może być dostępna wyłącznie po najechaniu.** Na telefonie najechania nie ma.
- **Każda akcja z prawego przycisku musi mieć odpowiednik dotykowy** (długie przytrzymanie).
  Dotyczy zwłaszcza menu kontekstowego w menedżerze plików.
- **Każdy skrót klawiszowy musi mieć odpowiednik dotykowy.** `Super+←/→` → przesunięcie palcem.
  `Super+D` („Pokaż pulpit", D-003) → gest od dolnej krawędzi albo przycisk. Bez tego na telefonie
  slider staje się nieosiągalny, gdy otwarta jest jedna aplikacja.

### 3.2 Rozróżnienie: animacja dekoracyjna ≠ manipulacja bezpośrednia **[do M3]**
Specyfikacja mówi „brak animacji przejść, snap". Na klawiaturze i myszy to jest słuszne
i zostaje bez zmian. Na dotyku **przesunięcie palcem, przy którym zawartość nie idzie za palcem,
sprawia wrażenie zepsutego** — nie wiesz, czy gest został w ogóle zarejestrowany.

Rozróżnienie do przyjęcia:
- **Animacja dekoracyjna** (przenikanie, odbicia, efekty) — nadal zakazana.
- **Manipulacja bezpośrednia** (karta podąża za palcem 1:1, po puszczeniu wskakuje na miejsce) —
  **wymagana** przy dotyku. To nie ozdoba, tylko informacja zwrotna.

Nie narusza to celu „zero renderowania w spoczynku": renderujemy w trakcie ruchu palca,
czyli dokładnie wtedy, gdy coś się dzieje. W spoczynku nadal zero.

### 3.3 Skalowanie — z „przezorności" staje się wymogiem **[już zdecydowane, D-011]**
Ekrany telefonów mają wysoką gęstość (Moto G8: 6,4" przy 1560×720). Bez skalowania UI jest
nieczytelnie małe. Decyzja D-011 (layout w jednostkach logicznych, skala mnożona przy rasteryzacji)
przestaje być zapasem na przyszłość — bez niej telefon jest nieosiągalny.
Dochodzi **`fractional-scale-v1`**, bo skale telefonowe rzadko są całkowite.

### 3.4 Orientacja pionowa i obrót ekranu **[do M3]**
Trzy strefy działają w pionie bez zmian, ale:
- **Reguła responsywna dla skrawka sąsiedniej karty** (przegląd §4.3) przestaje być kosmetyką.
  Przy 720 px szerokości dwa skrawki zjadają ekran — poniżej progu skrawek wyłączony,
  zastąpiony wskaźnikiem kropkowym.
- **Transformacja wyjścia (obrót)** musi być polem w modelu wyjścia od M1. Dorabianie obrotu
  do gotowego layoutu to przepisywanie layoutu.
- Do rozstrzygnięcia później: czy dolny pasek na telefonie ma się chować automatycznie —
  na 6" ekranie stały pasek przełącznika okien to kosztowna przestrzeń, zwłaszcza że
  na telefonie i tak zwykle jedna aplikacja jest na pełnym ekranie.

### 3.5 Zasilanie **[wzmocnienie istniejących celów]**
„Zero renderowania w spoczynku" na desktopie jest elegancją, na telefonie **decyduje o czasie
pracy na baterii**. Progi z D-017 zostają, ale dochodzą: wygaszanie ekranu, uśpienie, wybudzenie
(`idle-notify`, `idle-inhibit`, integracja z `logind`) — do M7, nie później.

---

## 4. Co może poczekać

- **Klawiatura ekranowa** — `text-input-v3` + `input-method-v2`. Bez niej na telefonie nie da się
  nic napisać, więc jest obowiązkowa, ale dopiero na etapie telefonu. **Nie pisać własnej** —
  `squeekboard` (z Phosh) jest gotową klawiaturą mówiącą `input-method-v2`. Nasza rola to
  zaimplementować protokół po stronie kompozytora i rezerwację miejsca na ekranie.
- **Port `rav`** — osobny projekt, patrz §2.
- **Waydroid** (aplikacje Androida) — dla użyteczności telefonu prawdopodobnie kluczowy, bo dobór
  natywnych aplikacji mobilnych na Linuksie jest ubogi. Stawia własne wymagania wobec kompozytora.
  Do rozstrzygnięcia, gdy shell będzie działał — ale warto mieć świadomość, że „telefon z Linuksem"
  bez Waydroida ma bardzo ograniczone zastosowanie.
- **Modem, SMS, połączenia** — poza zakresem, patrz §7 (świadoma decyzja, nie przeoczenie).

---

## 5. Zmiana w harmonogramie

Harmonogram M0–M10 z `01-strategia-dev-test.md` **nie wymaga przestawienia**, bo PC i tak jest
krokiem pierwszym. Zmieniają się natomiast wymagania wewnątrz istniejących etapów:

| Etap | Co dochodzi z powodu celu telefonowego |
|---|---|
| M1 | transformacja wyjścia (obrót) i skala w modelu wyjścia — pola obecne, choćby nieużywane |
| M2 | `wl_touch` jako osobna ścieżka wejścia obok `wl_pointer` |
| M3 | manipulacja bezpośrednia przy przesuwaniu kart; reguły responsywne; cele dotykowe ≥ 48 px; dotykowe odpowiedniki `Super+←/→` i `Super+D` |
| M7 | `idle-notify`, `idle-inhibit`, wygaszanie i uśpienie ekranu |
| M10 | `text-input-v3` + `input-method-v2` + integracja `squeekboard`; port `willow`, potem `rav` |

Dodatkowo warto wcześnie zdobyć **ekran dotykowy do PC lub RPi** — pozwala testować wszystko
z sekcji 3.1 i 3.2 bez czekania na telefon.

---

## 6. Tryb wskaźnika — telefon jako komputer

Ustalenie z 2026-07-30, kluczowe dla całego kierunku projektu: **celem nie jest „shell mobilny",
tylko zrobienie z telefonu komputera.** Wzorzec pochodzi z aplikacji RDP na telefonie, gdzie
przesuwanie palcem porusza kursorem po zdalnym pulpicie — palec działa jak gładzik w laptopie,
a nie jak palec dotykający ikony.

To rozwiązuje realny problem, którego dotyk bezpośredni nie rozwiązuje. Aplikacje desktopowe
(w tym wszystko, co przyjdzie przez XWayland) są projektowane pod kursor: mają małe cele,
reagują na najechanie, mają prawy przycisk i przeciąganie. Dotykiem bezpośrednim obsługuje się
je fatalnie. **Wirtualny gładzik przywraca precyzję co do piksela i wszystkie funkcje myszy
na ekranie, który ma 6 cali.**

### 6.1 Podział ról — to jest sedno

| Co obsługujesz | Czym |
|---|---|
| **Własny UI GostUI** (slider, paski, menu, menedżer plików) | **dotyk bezpośredni** — duże cele, przesunięcia, długie przytrzymanie |
| **Obce aplikacje desktopowe** (przez XWayland i natywne) | **tryb wskaźnika** — wirtualny gładzik |
| Mysz i klawiatura Bluetooth | normalnie, `wl_pointer` / `wl_keyboard` bez zmian |

Dzięki temu nie trzeba zmuszać obcych aplikacji do bycia dotykowymi (co jest niewykonalne),
ani robić własnego UI myszkowym (co byłoby złe na telefonie).

### 6.2 Wymagania implementacyjne

- **Tryb włączany świadomie przez użytkownika**, nie zgadywany. Przełącznik musi być natychmiastowy
  i zawsze dostępny — kandydat: przycisk w dolnym pasku albo gest dwoma palcami od krawędzi.
  Do rozstrzygnięcia w M3.
- **Kompozytor syntezuje `wl_pointer` ze zdarzeń dotyku** — ruch **względny** (jak gładzik),
  nie bezwzględny. Palec przesunięty o 1 cm przesuwa kursor o `1 cm × czułość`, a podniesienie
  i postawienie palca gdzie indziej **nie przenosi kursora**. To jest cała różnica między
  gładzikiem a ekranem dotykowym.
- **Kompozytor musi rysować kursor.** Na telefonie normalnie go nie ma; skoro i tak komponujemy
  obraz, dorysowanie kursora jest tanie. Kursor musi zostawać na miejscu po podniesieniu palca —
  dzięki temu **działa najechanie (hover)**, którego dotyk bezpośredni nie daje.
- **Pełny zestaw akcji myszy:**

  | Akcja myszy | Gest |
  |---|---|
  | ruch kursora | przesunięcie jednym palcem |
  | lewy przycisk | krótkie stuknięcie |
  | prawy przycisk | stuknięcie dwoma palcami **lub** długie przytrzymanie |
  | przeciąganie (drag) | stuknięcie i przytrzymanie, potem ruch — plus **blokada przeciągania** dla długich operacji |
  | przewijanie | przesunięcie dwoma palcami |
  | środkowy przycisk | stuknięcie trzema palcami (opcjonalnie) |

- **Blokada przeciągania** jest ważniejsza, niż się wydaje: przeciągnięcie pliku między panelami
  menedżera przez cały ekran jednym ruchem palca się nie uda. Potrzebny tryb „chwyć — puść palec —
  dosuń — upuść".
- **Czułość i przyspieszenie** konfigurowalne. Bez przyspieszenia przejście kursorem przez cały
  ekran wymaga kilku przesunięć.
- **Protokoły:** `relative-pointer-v1` i `pointer-constraints-v1` przestają być pozycją „dla gier"
  z tabeli protokołów (przegląd §3.7) i stają się częścią rdzenia wejścia.

### 6.3 Wpływ na resztę projektu

- **XWayland (M5) awansuje.** Skoro celem jest instalowanie i używanie normalnych programów,
  większość z nich będzie aplikacjami X11. XWayland przestaje być „odblokowaniem Warstwy 2",
  a staje się warunkiem podstawowego zastosowania systemu.
- **Dolny pasek zyskuje na znaczeniu.** Przy telefonie-jako-telefonie jedna aplikacja na pełnym
  ekranie wystarcza; przy telefonie-jako-komputerze przełączanie okien jest codzienną czynnością.
  Rozważane wcześniej automatyczne chowanie dolnego paska (§3.4) staje się wątpliwe.
- **Klawiatura Bluetooth zmienia bilans.** Z podłączoną klawiaturą skróty (`Super+←/→`, `Super+D`)
  wracają jako główna nawigacja, a klawiatura ekranowa schodzi na drugi plan. System musi działać
  dobrze w **obu** trybach i płynnie między nimi przechodzić.

---

## 7. Świadomie wyłączone podsystemy — i co to zmienia

Ustalenie: **aparat, modem/SIM i GPS są wyłączane.** Zostają **Wi-Fi i Bluetooth.**

To nie jest drobiazg konfiguracyjny — **to najważniejsza dobra wiadomość dla wykonalności portu.**
Przy mainlinowaniu telefonu najwięcej pracy i najwięcej porażek generują dokładnie: modem
(protokół QMI, firmware, ATOMy zasilania), aparat (CAMSS, sterowniki sensorów, ISP) oraz GPS.
Skreślenie ich razem usuwa większość ryzyka.

**Zostaje do uruchomienia w porcie `rav`:**

| Podsystem | Uwagi |
|---|---|
| Wyświetlacz (DSI) | panel wymaga własnego wpisu w device tree — typowo najwięcej dłubania z tego, co zostaje |
| Dotyk | kontroler I²C, zwykle prosty |
| GPU Adreno 610 | `freedreno`, wsparcie istnieje |
| **Wi-Fi** | zostaje — sterownik + firmware |
| **Bluetooth** | zostaje — `hci_qca`, potrzebne do myszy i klawiatury, czyli **krytyczne dla celu z §6** |
| Dźwięk | przydatny, ale nie blokujący |
| Bateria i ładowanie | wymagane do sensownego użycia |
| USB | wymagane — także jako droga ratunku przy debugowaniu |
| Uśpienie / wybudzenie | wymagane dla czasu pracy |
| Pamięć masowa (UFS/eMMC) | wymagane |

Wniosek: port `rav` pozostaje osobnym projektem (§2), ale **jego zakres jest teraz znacznie
mniejszy, niż sugerowała pierwotna ocena.** Bluetooth awansuje z „miło mieć" do funkcji
krytycznej — bez niego nie ma myszy ani klawiatury, czyli nie ma komputera.

### 7.1 Ekran zewnętrzny — kwestia rozstrzygnięta (D-026)

Moto G8 ma USB-C w wersji **USB 2.0**, bez linii SuperSpeed, a Motorola „Ready For" (tryb pulpitu
na monitorze) działa tylko na serii Edge z mocniejszymi układami. Praktycznie oznacza to
**brak wyjścia obrazu przez USB-C**.

Decyzją użytkownika (2026-07-30) **wyjście obrazu staje się kryterium doboru telefonu**, a nie
cechą, z braku której się rezygnuje — kandydaci w §2.1. Zmienia to scenariusz docelowy:
telefon w **stacji dokującej** (monitor, klawiatura, mysz), z telefonem jako jednostką centralną.

Cztery rzeczy, które z tego wynikają dla kodu — wszystkie od M1, bo dorabianie ich później
oznacza przeróbkę modelu wyjść i layoutu:

1. **Wyjścia w kolekcji, każde z własną skalą i transformacją.** Nigdzie poza warstwą rysowania
   nie wolno założyć, że wyjście jest jedno.
2. **Gorące podłączenie i odłączenie.** Okna i kafelki muszą przeżyć zniknięcie wyjścia, na którym
   stały — przy stacji dokującej to zdarzenie codzienne. To także najczęstsze miejsce paniki
   kompozytorów, więc trafia do kryteriów akceptacji M2, nie do „później".
3. **Kafelkowanie liczone per wyjście.** Podział wzdłuż dłuższej osi (D-025) daje na ekranie
   telefonu kafelki jeden nad drugim, a na monitorze obok siebie — jednocześnie, w tej samej sesji.
   Limit kafelków też jest cechą wyjścia, nie urządzenia.
4. **Tryb wskaźnika (§6) nie znika po zadokowaniu** — przeciwnie, ekran telefonu staje się wtedy
   gładzikiem obsługującym kursor na monitorze. To jego najbardziej naturalne zastosowanie.

Tryb wskaźnika pozostaje przy tym potrzebny także niezadokowany: na 6,4-calowym ekranie precyzja
co do piksela jest jedynym sposobem obsłużenia aplikacji desktopowych.

---

## 8. System zapisywalny, z normalną instalacją programów

Ustalenie: **żadnego systemu tylko-do-odczytu ani obrazu niezmiennego (immutable).** Ma być normalny
system, na którym instaluje się programy menedżerem pakietów i pracuje się jak na komputerze.

To ustalenie jest zgodne z realiami — **postmarketOS nie jest systemem niezmiennym**, ma zwykły
zapisywalny system plików i normalny menedżer pakietów. Nie trzeba niczego obchodzić.

Dwa doprecyzowania:
- Na telefonie zawsze zostaje jedna partycja tylko-do-odczytu: **`/vendor` z firmware'em**
  (Wi-Fi, Bluetooth, GPU). To jest konieczne i normalne — nie ogranicza w żaden sposób instalowania
  programów.
- **Pamięć:** Moto G8 ma 64 GB, więc pełny system z aplikacjami desktopowymi mieści się bez problemu.

### 8.1 Otwarta kwestia: która dystrybucja na telefonie

Tu jest rozjazd, który trzeba rozstrzygnąć przed etapem telefonowym:

| | Podstawa | Pakiety | Wsparcie sprzętu |
|---|---|---|---|
| **postmarketOS** | Alpine | `apk` | najlepsza infrastruktura portów (pmaports), gotowy port `willow` |
| **Mobian / Debian** | Debian | `apt` | **te same pakiety co na PC**, ale porty głównie PinePhone/Librem — dla `rav` port trzeba zrobić samemu, bez pmaports |

Strona PC-towa jest od 2026-08-01 zamknięta: **D-034 przyjmuje Debiana** na PC i Raspberry Pi.
To **nie przesądza telefonu** — postmarketOS ma całą maszynerię portowania, której Mobian dla `rav`
nie ma. Rozstrzygnięcie nie jest pilne (dotyczy M10), ale warto je podjąć **przed** rozpoczęciem
portu, bo określa, w jakim ekosystemie ten port powstaje. To jedyna pozostałość po pytaniu
o podstawę systemu.

Dobra wiadomość: **GostUI nie zależy od wyboru** — to zwykły program w Rust, który zbuduje się
i na Alpine, i na Debianie. Różnica dotyczy pakowania i zależności systemowych, nie kodu.

Uwaga praktyczna, gdyby wyszedł postmarketOS: różnica nie kończy się na `apk` zamiast `apt` —
dochodzi **OpenRC zamiast systemd**, a więc inny sposób zarządzania usługami. Panel sterowania
i menedżer usług są przed tym osłonięte adapterem z **D-037**; poza nim nic w UI nie ma prawa
wołać `systemctl` ani `apt` bezpośrednio.

Różnicą, która mogłaby zaboleć na pececie, ale nie na telefonie, jest **musl zamiast glibc**:
programy dostarczane wyłącznie jako gotowe binaria (część oprogramowania firmowego, niektóre
klienty) na Alpine się nie uruchomią. Na telefonie to marginalne, bo repertuar i tak jest budowany
ze źródeł — ale to jeden z powodów, dla których na PC Alpine nie wchodził w grę (D-034).
