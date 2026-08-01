# Zasoby graficzne

Grafiki przeniesione z poprzedniej wersji projektu (dystrybucja live-build, tag `v0-live-build-iso`).
Jedyna część tamtej pracy, która przechodzi do shella.

| Plik | Wymiary | Przeznaczenie |
|---|---|---|
| `logo.png` | 1272×1307 | logo projektu, README, ekran powitalny |
| `gostos-start.png` | 150×150 | ikona **[PROGRAMY]** w górnym pasku (Menu Start) |
| `avatar.png` | 800×800 | domyślny awatar użytkownika — greeter, panel systemowy |
| `wallpaper_gostos.jpg` | — | tło slidera kart |
| `login-background.png` | 6000×3375 | tło ekranu logowania (greeter) |
| `gostos-grub-background.png` | 6000×3375 | tło GRUB — przydatne dopiero przy powrocie do dystrybucji |

## Uwagi techniczne

- Dwa tła mają 6000×3375 px przy docelowym ekranie 1920×1080. Przed użyciem w kompozytorze
  trzeba przygotować warianty w rozdzielczościach docelowych — dekodowanie 20 Mpx PNG przy każdym
  starcie sesji jest sprzeczne z założeniem „ultralekki" ze specyfikacji. Oryginały zostają
  jako materiał źródłowy.
- `gostos-start.png` jest rastrem 150×150. Do paska systemowego, który ma obsługiwać skalowanie
  (D-011), przyda się wersja **SVG** albo komplet rozmiarów (16/24/32/48/64/128).
- Motyw ikon **WhiteSur** z poprzedniego repo nie został przeniesiony — to motyw firm trzecich
  (3744 pliki, 24 MB), dostępny w upstreamie. GostUI potrzebuje motywu ikon, ale jako
  **zależności instalowanej z pakietu**, nie plików wersjonowanych tutaj.
