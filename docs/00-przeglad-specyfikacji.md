# Przegląd specyfikacji GostUI — użyteczność i funkcjonalność

Dokument recenzuje `gostos.md`. Nie zmienia specyfikacji — zbiera ustalenia do decyzji.
Legenda wagi: **[BLOKER]** = uniemożliwia działanie założonego zakresu · **[LUKA]** = brakujący element,
bez którego coś nie będzie użyteczne · **[RYZYKO]** = kosztowna pomyłka do wyłapania teraz ·
**[OK]** = decyzja dobra, zostawić.

---

## 1. Ocena ogólna

Specyfikacja jest nietypowo dobra jako dokument produktowy: ma jasną tezę (trzy strefy, brak
mylenia UI systemu z UI aplikacji), świadomie odrzuca funkcje (brak animacji, minimalizm),
i — najważniejsze — sekcja 4 poprawnie dzieli odpowiedzialność między gotowe demony i własny kod.
Większość projektów tego typu tonie właśnie tam: próbują pisać własną obsługę sieci i dźwięku.
Tutaj tego nie ma i to jest fundament, na którym da się budować.

Braki są dwojakiego rodzaju:

1. **Stack techniczny ma dwa realne błędy** (wgpu vs. smithay, wgpu vs. RPi3) — obie kosztują
   tygodnie, jeśli wyjdą po fakcie. Sekcja 2.
2. **Warstwa "sprawia, że obce aplikacje w ogóle działają" jest prawie nieopisana** — XWayland,
   schowek, powiadomienia, tray, portale, polkit. To nie są bajery; bez nich Firefox nie skopiuje
   tekstu, a Menedżer usług nie zrestartuje usługi. Sekcja 3.

Reszta to doprecyzowania modelu interakcji (sekcja 4), które są tanie teraz i drogie później.

---

## 2. Stack technologiczny — dwie poprawki

### 2.1 `wgpu` jako renderer kompozytora **[RYZYKO — rekomendacja: zmienić]**

`smithay` ma wbudowane własne renderery (GLES2 oraz Pixman/CPU) i — co ważniejsze — wbudowaną
obsługę **importu buforów klienta** (`wl_shm`, `linux-dmabuf`), zarządzanie damage tracking
i kompozycję wyjścia. To jest 80% roboty kompozytora, która "jest już zrobiona", ale tylko dla
rendererów smithaya.

Wybierając `wgpu` bierzesz na siebie interop: bufor dostarczony przez klienta (dmabuf z jego GPU)
musisz samodzielnie zaimportować jako teksturę wgpu. Efekt praktyczny: **zanim wyświetlisz pierwsze
okno obcej aplikacji, piszesz warstwę interopu dmabuf↔wgpu.** Przy rendererze GLES smithaya
pierwsze okno pojawia się w kilka godzin.

Rekomendacja:
- v1: renderer GLES2 ze smithaya (`GlesRenderer`) — pełna ścieżka do klientów działa od razu.
- Dodatkowo od początku włączyć renderer **Pixman (CPU)** jako drugi backend. Nie dla wydajności,
  ale bo daje deterministyczny render bez GPU → testy zrzutów ekranu w CI oraz fallback dla
  słabego sprzętu (patrz 2.2).
- `wgpu` traktować jako opcjonalny eksperyment po M5, nie jako fundament.

### 2.2 Raspberry Pi 3 + `wgpu` są niekompatybilne **[BLOKER dla celu RPi3]**

RPi3 ma VideoCore IV (Mesa, sterownik `vc4`), który daje **OpenGL ES 2.0**. Backend GL w `wgpu`
wymaga **GLES 3.0** jako minimum. Czyli: na RPi3 `wgpu` nie wystartuje w ogóle, a nie "będzie
działać wolno". Vulkan na RPi3 nie istnieje.

Opcje, do wyboru:
- **RPi4/RPi5 zamiast RPi3** (VideoCore VI/VII, sterownik `v3d`, GLES 3.1) — cel zachowany, sprzęt zmieniony.
- **RPi3 z rendererem CPU (Pixman)** — realne właśnie dlatego, że specyfikacja zakłada brak animacji
  i zero renderowania w spoczynku. Statyczny UI 2D bez efektów to scenariusz, w którym renderer
  softwarowy jest wystarczający. Wymaga jednak, by renderer był abstrakcją od M1, a nie dopisaną potem.
- **Skreślić RPi3.**

Niezależnie od wyboru: renderer GLES2 smithaya *działa* na RPi3, więc jeśli rezygnujesz z wgpu
(2.1), problem znika sam. To dodatkowy argument za zmianą z 2.1.

### 2.3 `fontdue`/`ab_glyph` to za mało **[LUKA]**

Oba są rasteryzerami glifów — dostajesz bitmapę pojedynczego znaku. Brakuje trzech warstw:
odnajdywanie fontów w systemie (fontconfig), shaping (kerning, ligatury), oraz łamanie linii.
Pisanie tego samemu to tygodnie, a objawy są podstępne: polskie diakrytyki działają, ale kerning
i emoji nie.

Rekomendacja: **`cosmic-text`** — spina `fontdb` (odnajdywanie) + `rustybuzz` (shaping) +
`swash` (rasteryzacja) + łamanie linii i edycję tekstu w jednym API. To jest ta sama biblioteka,
z której korzysta COSMIC DE, więc jest sprawdzona dokładnie w tym zastosowaniu.

### 2.4 Ikony — nieobecne w specyfikacji **[LUKA]**

Cały UI (Menu Start, karty, menedżer plików, panel sterowania) opiera się na ikonach, a specyfikacja
nie mówi, skąd się biorą. Potrzebne:
- wyszukiwanie ikony po nazwie zgodnie z freedesktop Icon Theme Spec (`freedesktop-icons` crate),
- rendering SVG (`resvg`/`usvg`) — większość współczesnych motywów jest w SVG,
- cache zrasteryzowanych ikon per rozmiar (inaczej otwarcie Menu Start rasteryzuje 200 SVG).

Do dodania do stacku i do M3.

### 2.5 Greeter w warstwie Core **[RYZYKO — rekomendacja: przenieść]**

Greeter to *drugi kompozytor*: własna sesja, własny render, własna obsługa wejścia, uruchamiany
przed sesją użytkownika. Duplikuje najtrudniejszą część projektu, a daje najmniej — ekran logowania
widzisz 3 sekundy dziennie.

Rekomendacja: wyrzucić z Core. Na czas developmentu autologin + uruchamianie kompozytora
z usługi systemd użytkownika. Jeśli potrzebny wybór sesji — `greetd` + `tuigreet` (gotowy,
tekstowy, działa). Własny frontend greetd dopisać jako ostatni milestone, gdy renderer i tak
będzie już wyekstrahowany jako biblioteka do ponownego użycia.

### 2.6 Nazewnictwo

Specyfikacja mówi `GostUI`, katalog projektu to `GostOs`. To shell, nie system operacyjny —
`GostUI` jest trafniejsze. Warto ujednolicić przed pierwszym commitem (nazwa trafi do crate'ów,
ścieżek konfiguracji `~/.config/gostui/`, plików `.desktop` sesji).

---

## 3. Braki, które blokują działanie obcych aplikacji

Ta sekcja jest najważniejsza. Warstwa 2 (przeglądarka, RDP, Moonlight, C64) to w praktyce nie
"funkcje do napisania", a **obce aplikacje do uruchomienia**. Ich koszt to zero, *pod warunkiem*
że kompozytor dostarcza to, czego oczekują. Poniżej lista tego, czego specyfikacja nie wymienia.

### 3.1 XWayland **[BLOKER dla Warstwy 2]**

VICE (emulator C64), wiele klientów RDP, Moonlight, starsze narzędzia — to aplikacje X11.
Bez XWayland nie wystartują wcale. `smithay` to wspiera, ale nie "za darmo": trzeba obsłużyć
osobny cykl życia serwera, mapowanie okien X11 na własny model okien, zarządzanie zaznaczeniem
(schowek X11 ↔ Wayland) i override-redirect windows (menu, tooltipy).

Wniosek strategiczny: **XWayland jest warunkiem wstępnym całej Warstwy 2, więc jest tańsze niż
cokolwiek z Warstwy 2 i powinno je poprzedzać.** W harmonogramie: zaraz po działającym
window managemencie.

### 3.2 Schowek (`wl_data_device`) **[BLOKER]**

Bez implementacji `wl_data_device_manager` **kopiuj-wklej między aplikacjami nie istnieje**.
Nie działa źle — nie ma go. To pierwsza rzecz, którą użytkownik wykryje po 30 sekundach.
Do tego `primary-selection` (wklejanie środkowym przyciskiem, standard na Linuksie).
Musi być w tym samym milestone co xdg-shell, nie później.

### 3.3 Demon powiadomień (`org.freedesktop.Notifications`) **[LUKA — awansować do Warstwy 1]**

Aplikacje wysyłają powiadomienia przez D-Bus do tego, kto zajmie tę nazwę. Jeśli nikt jej nie
zajmuje, aplikacje dostają błąd D-Bus — część zaloguje ostrzeżenie, część się wywali.
Specyfikacja nie wspomina o powiadomieniach ani słowem. To nie bajer: to kontrakt, który shell
musi spełnić. Minimalna implementacja (jeden dymek w narożniku, kolejka, timeout) to ~dzień pracy.

### 3.4 Tray / `StatusNotifierItem` **[LUKA]**

Bez traya aplikacje, które chowają się do zasobnika (komunikatory, Steam, klienty synchronizacji,
Nextcloud), po zamknięciu okna **znikają bez śladu i nie ma jak ich przywrócić**. Skoro w Warstwie 2
są "komunikatory", tray jest ich warunkiem użyteczności. Miejsce naturalne: prawa strona górnego
paska, obok ikony `[SYSTEM]`.

### 3.5 Portale (`xdg-desktop-portal`) **[LUKA — tania]**

Przeglądarki i aplikacje Electron/Flatpak używają portali do: okna wyboru pliku, udostępniania
ekranu, otwierania URI. Bez backendu portalu udostępnianie ekranu (videorozmowy) nie zadziała.
Rozwiązanie na v1 jest tanie: zainstalować `xdg-desktop-portal` + `xdg-desktop-portal-gtk`
i poprawnie ustawić `XDG_CURRENT_DESKTOP=GostUI` w pliku sesji. Nie trzeba pisać własnego backendu.

### 3.6 Polkit **[LUKA — dotyczy Menedżera usług i Panelu sterowania]**

`systemctl start/stop/restart` przez D-Bus wymaga autoryzacji polkit. Jeśli w sesji nie działa
**agent uwierzytelniania polkit**, każda akcja w Menedżerze usług zwróci nieczytelny błąd
"not authorized" i nic więcej. To samo dotyczy zmiany daty/godziny i kont użytkowników
w Panelu sterowania.

Opcje: uruchamiać w sesji gotowy agent (np. `polkit-gnome` / `lxpolkit` / `mate-polkit` —
na tej maszynie już działa `polkit-mate-authentication-agent-1`), albo napisać własny agent
(spójny wizualnie, ale to dodatkowa praca z D-Bus). Rekomendacja v1: gotowy agent, uruchamiany
przez sesję GostUI.

### 3.7 Lista protokołów Wayland do zaimplementowania **[LUKA — brak w specyfikacji]**

Specyfikacja nie zawiera listy protokołów, a to ona definiuje "czy aplikacja X działa".
Minimum do uznania kompozytora za używalny:

| Protokół | Bez tego nie działa |
|---|---|
| `xdg-shell` | okna aplikacji w ogóle |
| `wl_seat` (pointer/keyboard/touch) | wejście |
| `wl_shm` + `linux-dmabuf` | bufory klientów (CPU i GPU) |
| `wl_output` + `xdg-output` | rozdzielczość, skalowanie, wiele wyjść |
| `wl_data_device` + `primary-selection` | schowek (3.2) |
| `xdg-decoration` | negocjacja ramek okien — inaczej Qt rysuje własne ramki, GTK swoje, chaos |
| `xdg-activation` | "podnieś okno" po kliknięciu linku |
| `cursor-shape` | kursory bez rasteryzacji po stronie klienta |
| `viewporter` + `presentation-time` | odtwarzanie wideo bez artefaktów |
| `fractional-scale` | HiDPI (4.9) |
| `pointer-constraints` + `relative-pointer` | gry, Moonlight, RDP (przechwycenie myszy) |
| `idle-inhibit` + `idle-notify` | wygaszanie ekranu / blokada przy filmie |
| `wlr-screencopy` lub portal | zrzuty ekranu (potrzebne też do testów!) |
| `text-input-v3` + `input-method-v2` | klawiatura ekranowa — warunek celu "telefon" |

Do rozstrzygnięcia świadomie: `wlr-layer-shell` — potrzebny tylko, jeśli chcesz kiedyś używać
zewnętrznych paneli (waybar, wofi). Skoro paski piszesz sam, można pominąć w v1, ale wsparcie
otwiera drogę do gotowych narzędzi awaryjnie.

---

## 4. Model interakcji — luki do doprecyzowania

### 4.1 Relacja "slider ↔ otwarte okna" jest niezdefiniowana **[LUKA — najważniejsza w tej sekcji]**

Specyfikacja mówi, że aplikacje otwierają się "na środkowym obszarze ekranu (nad sliderem)".
Nie odpowiada na pytania, które pojawią się w pierwszej minucie użytkowania:

- Mam otwarte okno. Przełączam kartę strzałką. **Co się dzieje z oknem?** (znika? zostaje? karta
  zmienia się niewidocznie pod oknem?)
- Mam otwarte okno na pełnym obszarze. **Jak wrócić do slidera bez minimalizowania wszystkiego
  po kolei z dolnego paska?**
- Czy karty są związane z oknami (jak wirtualne pulpity), czy zupełnie niezależne?

To jest rozwidlenie architektoniczne, nie detal. Dwa spójne modele:

**Model A — slider to warstwa pulpitu (rekomendowany).** Karty nie mają nic do okien.
Okna zawsze zakrywają slider. Zmiana karty pod oknem jest bezstanowa i niewidoczna.
Potrzebna jest **jedna akcja "Pokaż pulpit"** (klawisz, np. `Super+D`, plus kafelek/ikona na
dolnym pasku) chowająca wszystkie okna. Proste do implementacji, zero niespodzianek.

**Model B — karty to wirtualne pulpity.** Okno należy do karty, na której je otwarto; zmiana
karty przełącza zestaw widocznych okien. Mocniejsze (karta "Projekt Python" = terminal + edytor
+ te konkretne okna), ale wymaga: przypisania okna do karty, przenoszenia okien między kartami,
oraz decyzji, czy dolny pasek pokazuje okna wszystkich kart czy tylko bieżącej.

Model B jest bliższy sformułowaniom w specyfikacji ("karta tematyczna: terminal + folder z kodem
+ dokumentacja"), ale Model A jest zdecydowanie tańszy. **Do rozstrzygnięcia przed M2.** Jeśli B —
dolny pasek musi mieć wyraźne rozdzielenie okien bieżącej karty i pozostałych, inaczej użytkownik
gubi okna.

### 4.2 Strzałki lewo/prawo jako nawigacja kart **[BLOKER użyteczności]**

Jeśli strzałki przełączają karty globalnie, **każde pole tekstowe w każdej aplikacji przestaje
działać** (nie przesuniesz kursora w edytorze). Konieczne jedno z dwóch:
- klawisze z modyfikatorem globalnie: `Super+←/→` (rekomendacja — działa też przy aktywnym oknie),
- gołe strzałki działają *tylko* gdy fokus ma slider, a nie okno aplikacji.

Rekomendacja: oba. `Super+←/→` globalnie, gołe strzałki gdy slider ma fokus.
Przy okazji: warto od razu zdefiniować pełną mapę globalnych skrótów jako jedną tabelę
w konfiguracji, a nie rozsianą po kodzie.

### 4.3 Skrawek sąsiedniej karty przy małych ekranach **[RYZYKO]**

"Sąsiednie karty częściowo widoczne po bokach" jest dobre na 1920px. Na RPi3 przy 1280px,
a zwłaszcza na telefonie w pionie (720px), dwa skrawki zjadają nieproporcjonalną część ekranu.
Potrzebna reguła responsywna, np.: skrawek = `min(48px, 3% szerokości)`, a poniżej 800px szerokości
skrawek wyłączony i zastąpiony wskaźnikiem kropkowym (jak w karuzeli). Do zapisania jako reguła
layoutu, nie do improwizacji w kodzie.

### 4.4 Karta przypięta — nierozstrzygnięte trzy rzeczy **[LUKA]**

- Czy okna aplikacji **zakrywają** panel przypięty, czy panel rezerwuje przestrzeń (jak paski)?
  Rekomendacja: rezerwuje. Inaczej "zawsze widoczny" jest nieprawdą.
- Ile kart można przypiąć? Rekomendacja v1: **jedna**. Wiele przypiętych paneli = system układania
  okien od zera.
- Czy przypięta karta znika ze slidera, czy jest w nim nadal? Rekomendacja: znika (jest gdzie
  indziej, dublowanie myli).

### 4.5 Czym jest zawartość karty **[LUKA — chroni przed rozjazdem zakresu]**

"Karta PRACA — edytory tekstu, arkusze, klient RDP, kalkulator" da się przeczytać dwojako:
karta zawiera *skróty uruchamiające* te aplikacje (tanie), albo karta *zawiera osadzone widoki*
tych aplikacji (osadzanie obcych okien w kontenerze — bardzo drogie i w Waylandzie zasadniczo
niedostępne).

Do jawnego zapisania: **karta = siatka elementów (skrót do aplikacji / skrót do folderu /
zamontowany dysk / plik)**. Nic więcej. Bez tego zdania ktoś kiedyś spróbuje osadzać okna.

### 4.6 Menu Start z folderu na dysku — świetny pomysł, brakuje trzech mechanizmów **[OK + LUKA]**

Pomysł jest bardzo dobry: znika edytor menu, znika baza danych menu, użytkownik zarządza menu
menedżerem plików, który i tak piszesz. Zostawić. Brakuje jednak:

- **Bootstrap.** Przy pierwszym uruchomieniu `~/gostui/menu_start/` jest pusty → menu jest puste →
  system wygląda na zepsuty. Potrzebny generator, który przy pierwszym starcie zaczyta
  `/usr/share/applications/*.desktop` i `~/.local/share/applications/` i rozłoży je po folderach
  na podstawie pola `Categories`.
- **Nowo instalowane aplikacje.** `apt install gimp` nie doda niczego do `menu_start/`.
  Potrzebny `gostui-menu-sync`: skanuje katalogi systemowe, nowe pozycje wrzuca do
  `Nieskategoryzowane/`, **nigdy nie rusza tego, co użytkownik już poukładał** (stan "co już
  widziałem" w osobnym pliku). Uruchamiany przy starcie sesji i na żądanie.
- **Odświeżanie na żywo.** Bez `inotify` na drzewie `menu_start/` zmiany zrobione w menedżerze
  plików nie pojawią się w menu do restartu sesji.

Dodatkowo: skróty jako **symlinki** do plików systemowych `.desktop`, nie kopie — wtedy aktualizacja
pakietu aktualizuje wpis. I `.desktop` to trafny wybór (potwierdzić z sekcji 5 specyfikacji), ale
trzeba obsłużyć **kody pola w `Exec`** (`%f %F %u %U %i %c %k`) — bez ich usunięcia/podstawienia
polecenia uruchomienia się wywalą; oraz `Terminal=true` (uruchom w emulatorze terminala),
`NoDisplay=true` (ukryj), `TryExec`, `Hidden`.

### 4.7 Wyszukiwanie aplikacji tylko po kliknięciu ikony **[LUKA użyteczności]**

Uruchamianie aplikacji przez: klik w ikonę lupy → klik w pole → wpisanie nazwy, jest wolniejsze
niż `Alt+F2` czy terminal. Efekt: nikt tego nie użyje. Potrzebny **skrót klawiszowy otwierający
wyszukiwanie od razu z fokusem w polu** (`Super` samo lub `Super+S`), Enter uruchamia pierwszy wynik.
Dopasowanie powinno objąć też pola `Comment` i `Keywords` z `.desktop` (użytkownik szuka "przeglądarka",
nie "firefox").

### 4.8 Dolny pasek — brakujące reguły **[LUKA]**

- **Które okna dostają kafelek?** Tylko `xdg_toplevel`. Popupy, menu, tooltipy, okna dialogowe
  potomne — nie (inaczej rozwinięcie menu w Firefoksie dodaje kafelek).
- **Zamykanie z paska.** Użytkownicy oczekują zamknięcia okna z kafelka (środkowy przycisk lub
  menu kontekstowe). Brak tego jest odczuwalny.
- **Przepełnienie.** 20 otwartych okien na 1920px: kafelki się zwężają do nieczytelności?
  Przewijanie? Grupowanie po aplikacji? Trzeba wybrać regułę.
- **Sygnalizacja "aplikacja się czegoś domaga"** (`xdg-activation`) — podświetlenie kafelka.

### 4.9 Skalowanie (HiDPI) i wiele monitorów **[RYZYKO — decyzja teraz, implementacja później]**

Ani skalowanie, ani wiele wyjść nie są wspomniane. Oba są bardzo drogie do dorobienia po fakcie
we własnoręcznie pisanym rendererze i layoucie.

Rekomendacja:
- **Skalowanie:** od M1 cały layout liczyć w **jednostkach logicznych**, a skalę przechowywać
  per wyjście i mnożyć na końcu, przy rasteryzacji. Nawet jeśli w v1 skala jest zawsze 1.0.
  Koszt teraz: prawie zero. Koszt później: przepisanie całego layoutu. Cel "telefon" to wymusza.
- **Wiele monitorów:** jawnie **poza zakresem v1** (ta maszyna ma jeden HDMI 1920x1080), ale kod
  obsługi wyjść trzymać w kolekcji, nie w jednym polu `output`. Do rozstrzygnięcia przy porcie:
  paski na każdym wyjściu czy tylko na głównym.

### 4.10 Regulacja jasności na desktopie **[detal, ale widoczny od razu]**

Panel `[SYSTEM]` ma suwak jasności. Na tej maszynie **nie ma `/sys/class/backlight`** (monitor
HDMI, brak podświetlenia zarządzanego przez system) — suwak nie będzie miał na co działać.
Reguła: jeśli nie ma urządzenia backlight, **ukryć suwak**, a nie pokazywać nieaktywny.
Opcjonalnie fallback przez DDC/CI (`ddcutil`) — działa na monitorach zewnętrznych, ale wymaga
modułu `i2c-dev` i uprawnień; raczej nie w v1.

---

## 5. Menedżer plików — uwagi

### 5.1 Warstwa dostępu do plików (VFS) **[RYZYKO — decyzja przed pierwszą linią menedżera]**

Specyfikacja wymaga dysków lokalnych, nośników wymiennych **i dysków sieciowych SFTP** w jednym
widoku. Jeśli kod menedżera będzie operował na `std::path::Path` i `std::fs`, dodanie SFTP oznacza
przepisanie wszystkiego.

Rekomendacja: zdefiniować **wąski trait VFS** (`list`, `stat`, `read`, `write`, `mkdir`, `remove`,
`rename`, `copy_stream`) i adresowanie typu `(backend_id, ścieżka)` **od pierwszego commita**
menedżera, nawet gdy jedyną implementacją jest backend lokalny. To kilka godzin teraz.

Alternatywa tania: SFTP montować przez `sshfs` i widzieć jako zwykłą ścieżkę. Wtedy VFS niepotrzebny,
ale tracisz kontrolę nad błędami połączenia (a specyfikacja wprost wymaga obsługi zerwanego
połączenia komunikatem, nie zawieszeniem). Do rozstrzygnięcia — obie drogi są obronne,
`sshfs` jest szybszy do v1.

### 5.2 Hasła SFTP

Uwaga bezpieczeństwa z samej specyfikacji jest słuszna. Wzmocnienie: w v1 **obsługiwać wyłącznie
klucze SSH** i nie mieć w ogóle pola "hasło". Wtedy nie ma czego przechowywać, a keyring
(`secret-service`) dochodzi później jako funkcja, nie jako łatanie dziury.

### 5.3 Kosz musi być zgodny ze specyfikacją freedesktop **[LUKA]**

Własny kosz oznacza, że pliki usunięte w GostUI są niewidoczne dla innych narzędzi (i odwrotnie).
Zgodność: `~/.local/share/Trash/{files,info}` + plik `.trashinfo` z oryginalną ścieżką i datą.

Pułapka: **kosz działa tylko w obrębie tego samego systemu plików.** Usunięcie pliku z pendrive'a
do kosza w `$HOME` to fizyczne kopiowanie (przy 8 GB — minuty i zapełniony dysk). Reguła:
używać `.Trash-1000` na urządzeniu docelowym, a jeśli nie da się utworzyć — zapytać
o trwałe usunięcie. Do zapisania jawnie.

### 5.4 Wyszukiwanie rekurencyjne

"Przeszukuje bieżący folder i wszystko poniżej" — na `/` albo dużym dysku to minuty i zamrożone
okno. Wymagania niefunkcjonalne do dopisania: wyszukiwanie w wątku roboczym, wyniki strumieniowo
(pojawiają się w miarę znajdowania), przycisk anulowania, pomijanie `/proc`, `/sys`, `/dev`
i punktów montowania sieciowych bez zgody użytkownika.

### 5.5 Przeciąganie plików między panelami

Wewnątrz własnej aplikacji DnD jest łatwe (masz pełną kontrolę nad stanem). Między aplikacjami
(np. z menedżera plików do przeglądarki) potrzebny `wl_data_device` z pełną obsługą DnD, co jest
sporo trudniejsze od samego schowka. Rekomendacja v1: **DnD tylko wewnątrz menedżera plików**
(między panelami i do szybkiego dostępu), międzyaplikacyjne później.

### 5.6 Skojarzenia plików

Nie wymyślać własnego formatu mapowań — użyć istniejącego `mimeapps.list` (freedesktop MIME
Applications Spec) + `shared-mime-info` do rozpoznawania typu (rozpoznawanie po samym rozszerzeniu
jest zawodne). Efekt uboczny: skojarzenia współdzielone z resztą systemu, użytkownik konfiguruje
je raz.

---

## 6. Odporność i cechy niefunkcjonalne — nieobecne w specyfikacji

### 6.1 Awaria kompozytora = koniec sesji **[LUKA]**

W tym modelu kompozytor jest jednym procesem, od którego zależy wszystko. Panika w Rust przy obsłudze
jednego złośliwie sformułowanego żądania klienta = utrata wszystkich otwartych aplikacji.

Minimum:
- Nadzorca (usługa systemd użytkownika z `Restart=on-failure`), który restartuje kompozytor.
- **Zapis stanu** (karty, ich kolejność, szerokości, przypięcie) atomowo — zapis do pliku
  tymczasowego i `rename`. Bez tego pierwsza awaria kasuje konfigurację.
- Błąd protokołu ze strony klienta musi **zabijać klienta, nie kompozytor** — to reguła Waylanda;
  wymaga dyscypliny, by nigdzie w obsłudze żądań nie było `unwrap()`.
- W developmencie: zawsze uruchamiać zagnieżdżone w XFCE (patrz strategia testów) — awaria kosztuje
  wtedy jedno okno, nie sesję.

### 6.2 Mierzalne cele wydajności **[LUKA]**

Specyfikacja mówi "ultralekki" i "zero renderowania w spoczynku". Bez liczb to nie jest testowalne.
Propozycja progów akceptacji (do potwierdzenia):
- W spoczynku (bez wejścia, statyczny slider): **0 wyrenderowanych klatek na 10 s**, CPU < 1%.
- Zużycie RAM samego kompozytora + paski + slider: **< 120 MB RSS**.
- Od `exec` do widocznego slidera: **< 1 s** na tej maszynie.
- Opóźnienie zmiany karty (wejście → klatka na ekranie): **< 16 ms** (jedna klatka; "snap"
  ze specyfikacji jest tu wprost mierzalny).
- Test wytrzymałościowy: 24 h z otwartym Firefoksem i terminalem, bez wzrostu RSS i bez awarii.

### 6.3 Dostępność (a11y)

Realistycznie poza zakresem (AT-SPI to duża praca). Warto to zapisać jako **świadomą decyzję**,
nie przemilczenie — i przy okazji zachować rzeczy tanie: pełna obsługa klawiatury (jest w założeniach),
konfigurowalny rozmiar czcionki, poszanowanie preferencji kontrastu.

---

## 7. Rekomendowana zmiana kolejności prac

Specyfikacja (sekcja 6) proponuje: szkielet → statyczny render → window management → slider →
menedżer plików. Kolejność jest dobra, ale ma dwie słabości: pierwszy testowalny efekt jest dopiero
przy graficznym renderze, a XWayland i schowek nie mają miejsca.

Proponowana korekta, w trzech zasadach:

1. **Najpierw logika bez grafiki.** Model kart, parser `.desktop`, konfiguracja, ścieżki kosza,
   sortowanie — to wszystko da się napisać i przetestować testami jednostkowymi bez ani jednego piksela.
   Osobny crate `gostui-core` bez zależności od smithaya. Efekt: od pierwszego dnia jest co uruchamiać
   i co weryfikować, a największa część logiki nigdy nie wymaga kompozytora do testów.
2. **"Prawdziwy kompozytor" wcześniej niż slider.** Milestone „obca aplikacja (terminal) otwiera się,
   dostaje klawiaturę, działa kopiuj-wklej, ma kafelek na dolnym pasku" jest najważniejszym punktem
   weryfikacji ryzyka w całym projekcie. Wszystko po nim jest przewidywalne. Slider — mimo że jest
   sercem produktu — jest logiką na własnym renderze i nie odkryje żadnej niespodzianki.
3. **XWayland zaraz po tym**, bo odblokowuje całą Warstwę 2 za jeden koszt.

Szczegółowy harmonogram z kryteriami weryfikacji: `01-strategia-dev-test.md`.

---

## 8. Podsumowanie — do rozstrzygnięcia przed pierwszą linią kodu

Cztery pytania blokujące (odpowiedzi zmieniają architekturę):

1. **Renderer:** GLES2 smithaya + Pixman (rekomendacja), czy `wgpu`? → sekcja 2.1
2. **RPi3:** zamiana na RPi4/5, renderer CPU, czy skreślenie celu? → sekcja 2.2
3. **Karty vs. okna:** Model A (warstwa pulpitu, rekomendacja) czy Model B (wirtualne pulpity)? → sekcja 4.1
4. **Dostęp do plików:** własny trait VFS z backendem SFTP, czy `sshfs` i zwykłe ścieżki? → sekcja 5.1

Sześć rzeczy do dopisania do zakresu (żeby obce aplikacje działały): XWayland, schowek,
powiadomienia, tray, portale, agent polkit. → sekcja 3

Trzy poprawki taniego stacku: `cosmic-text` zamiast `fontdue`, biblioteka ikon + SVG,
greeter poza Core. → sekcje 2.3, 2.4, 2.5

Reszta to doprecyzowania do wpisania w specyfikację (sekcje 4–6) — żadna nie zmienia architektury,
wszystkie są tanie teraz.
