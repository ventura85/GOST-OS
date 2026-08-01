# Specyfikacja Projektu: GostUI — Nowy Shell/GUI

## 0. Założenia ogólne

- **Nazwa projektu:** GostUI
- **Cel:** ultralekki, autorski shell/desktop environment pisany w Rust, wykorzystujący Wayland, zaprojektowany od podstaw (inspiracja układem i prostotą klasycznych interfejsów, nie kopiowanie 1:1).
- **Platforma deweloperska:** Debian (minimalny, bez DE), bez migracji z dotychczasowego systemu roboczego.
- **Platformy docelowe:** PC x86_64 (stacja robocza), Raspberry Pi 3, ewentualnie stary smartfon (postmarketOS) — jako appliance.
- **Stack technologiczny (proponowany):**
  - Kompozytor/window management: `smithay` (Rust, framework do kompozytorów Wayland)
  - Rendering: `wgpu` (Vulkan/OpenGL ES pod spodem)
  - Fonty: `fontdue` / `ab_glyph`
  - Integracja z usługami systemowymi: `zbus` (D-Bus)
  - Greeter (ekran logowania): `greetd` + własny frontend graficzny

## 1. Podział na warstwy priorytetów

### Core (bez tego system nie działa)
- Kompozytor Wayland + window management
- Górny pasek (system bar)
- Slider kart (główna nawigacja)
- Dolny pasek (przełącznik okien)
- Ekran logowania (greeter)

### Warstwa 1 (codzienna użyteczność)
- Menedżer plików (styl Windows XP)
- Menedżer usług (styl Windows Services)
- Panel sterowania (styl Windows XP Control Panel)

### Warstwa 2 (integracje, nice-to-have)
- Klient RDP (FreeRDP)
- Przeglądarka internetowa (WebKit lekki / Firefox w oknie)
- Streaming gier (Moonlight/Steam Remote Play)
- Emulator C64 (najlepiej oparty o istniejący silnik, np. z VICE, zamiast pisania emulacji CPU od zera)

## 2. Anatomia ekranu — trzy strefy

Ekran podzielony na trzy niezależne, nienachodzące się strefy: górny pasek (system), środek (nawigacja/karty), dolny pasek (otwarte aplikacje). Cel: wyeliminowanie mylenia UI systemowego z UI aplikacji (np. kart przeglądarki).

### A. Górny pasek (System Bar)
Wyłącznie informacje i funkcje globalne, cztery elementy:
- **[PROGRAMY] — Menu Start** (lewo): menu w stylu Windows 98 — **dosłowna struktura folderów i skrótów**, nie płaska lista. Otwierasz menu, widzisz foldery kategorii, w folderach skróty do aplikacji (i ewentualnie zagnieżdżone podfoldery), dokładnie jak eksplorowanie drzewa katalogów.
  - **Backing na dysku:** menu odzwierciedla fizyczny folder, np. `~/gostui/menu_start/`. Podfoldery = kategorie w menu, pliki wewnątrz = skróty (np. w formacie `.desktop` — plik tekstowy z nazwą, ścieżką do ikony i komendą do uruchomienia).
  - Użytkownik może ręcznie zarządzać menu — dodawać/usuwać/przenosić pliki i foldery bezpośrednio w menedżerze plików, bez dedykowanego edytora menu.
- **Ikona wyszukiwania** (obok Menu Start): pojedyncza ikona, po kliknięciu otwiera pole wyszukiwania aplikacji (przeszukuje skróty z `menu_start/` po nazwie).
- **Zegar i data** (środek) — duży, minimalistyczny.
- **[SYSTEM]** (prawo) — jedna zbiorcza ikona statusu, po kliknięciu rozwija panel:
  - suwak głośności i jasności,
  - stan i wybór sieci Wi-Fi,
  - stan baterii,
  - przyciski: uśpij / uruchom ponownie / wyłącz.

### B. Środek ekranu — Slider kart
Główna nawigacja systemu, zastępuje klasyczny pulpit.

**Zachowanie:**
- Brak animacji przejść — przesunięcie (klik strzałki / klawisz) = natychmiastowa zmiana karty (snap).
- Nawigacja: klawiatura (strzałki lewo/prawo) oraz klik/dotyk.
- Sąsiednie karty częściowo widoczne po bokach (skrawek jako wizualna podpowiedź, że jest kolejna karta).
- Bez efektów 3D/perspektywy — płaski układ 2D, minimalny narzut GPU w spoczynku (statyczny widok = zero renderowania ciągłego).

**Zarządzanie kartami:**
- Tryb edycji: zmiana kolejności kart metodą przeciągania (drag) lub przycisków przesunięcia.
- **Przypinanie karty:** przypięta karta staje się stałym, zawsze widocznym panelem na ekranie (np. z boku); slider z pozostałymi kartami działa dalej w pozostałej części ekranu.
- Przycisk **[+] Nowa karta** — tworzenie własnych kart tematycznych (np. "Projekt Python": terminal + folder z kodem + dokumentacja).

**Domyślne karty tematyczne:**
- **[PLIKI]** — inteligentny pulpit: przypięte skróty do folderów, podmontowanych dysków, ulubionych plików.
- **[PRACA]** — edytory tekstu, arkusze, klient RDP, kalkulator.
- **[ROZRYWKA]** — emulator C64, Moonlight/Steam, odtwarzacz multimediów.
- **[INTERNET]** — przeglądarka, komunikatory.
- **[USTAWIENIA]** — narzędzia sieciowe, konfiguracja shella, diagnostyka sprzętu.

**Ikony funkcyjne na górze każdej karty:**
| Ikona | Działanie |
|---|---|
| Resize | Przeciąganie krawędzi karty — zmiana szerokości panelu (czysto strukturalne, rozmiar ikon/tekstu wewnątrz się nie zmienia) |
| Pokaż tylko ikony | Przełącznik widoku: ikony + podpisy ↔ same ikony (gęstszy układ) |
| Sortuj | Menu opcji sortowania zawartości karty (np. alfabetycznie, data, ręcznie) |

Stan każdej karty (szerokość po resize, tryb wyświetlania, sortowanie) jest **zapamiętywany osobno per karta** (potwierdzone).

### C. Dolny pasek (Bottom Bar)
- Każde otwarte okno aplikacji dostaje kafelek na dolnym pasku.
- Kliknięcie kafelka: minimalizuje okno (powrót do slidera) lub przywraca je na wierzch.
- Aplikacje otwierają się na środkowym obszarze ekranu (nad slidera), zostawiając dolny pasek zawsze widocznym do przełączania.
- Rozdzielenie odpowiedzialności: górny pasek = system, dolny pasek = przełącznik okien aplikacji — oko użytkownika nigdy nie myli jednego z drugim.

## 3. Aplikacje własne (Warstwa 1)

### Menedżer plików
- Styl inspirowany Windows XP Explorer: minimalizm, brak nadmiaru opcji.
- **Uruchomienie:** klik na ikonę "Mój komputer" (Home) → osobne okno.
- **Pierwszy widok (ekran główny):** lista zamontowanych zasobów, pogrupowana w **wyraźnie rozróżnione sekcje z nagłówkami**:
  - Dyski lokalne (np. `DISK_A` — pamięć wewnętrzna)
  - Nośniki wymienne (pendrive, karta SD)
  - Dyski sieciowe (dodawane ręcznie przez użytkownika, protokół SFTP)
- **Lewy pasek — Szybki dostęp (edytowalny):**
  - Domyślnie pusty lub z kilkoma sugestiami.
  - Dodawanie: przeciągnięcie folderu z prawego panelu lub opcja "Dodaj do szybkiego dostępu" z menu kontekstowego.
  - Usuwanie / zmiana kolejności: bezpośrednio w pasku (drag, lub opcja "Usuń").
- **Górny pasek narzędzi:**
  - Pasek adresu: **breadcrumb klikalny** (np. `Mój komputer > DISK_A > Dokumenty`, każdy segment klikalny) — domyślne rozwiązanie.
  - Ikona wyszukiwania — przeszukuje **bieżący folder i wszystko poniżej niego (rekurencyjnie)**. Np. w folderze głównym dysku przeszukuje cały dysk; wejście głębiej (np. do podfolderu) zawęża wyszukiwanie tylko do tego podfolderu i jego zawartości.
  - Przełącznik widoku: Lista / Małe ikony / Duże ikony.
  - Przycisk **[+ Dodaj dysk sieciowy]** — otwiera formularz: adres IP, login, hasło (protokół **SFTP**). Po zatwierdzeniu zasób zapisuje się trwale i pojawia na ekranie głównym przy kolejnych uruchomieniach.
  - **Uwaga bezpieczeństwa (rekomendacja):** hasła do zasobów sieciowych nie powinny być trzymane jako plain-text w pliku konfiguracyjnym — docelowo warto użyć systemowego keyringu (`secret-service` przez D-Bus) lub oprzeć się o klucze SSH zamiast hasła.
- Dwupanelowy tryb (do przeciągania plików między lokalizacjami / dyskami).

**Funkcje podstawowe (fundament, bez tego menedżer nie jest użyteczny):**
- Menu kontekstowe (prawoklik) na pliku/folderze: Otwórz, Kopiuj, Wklej, Usuń, Zmień nazwę, Właściwości, Dodaj do szybkiego dostępu, Nowy > Folder / Nowy > Dokument tekstowy.
- Skróty klawiszowe: Ctrl+C/V/X (kopiuj/wklej/wytnij), Delete, F2 (zmień nazwę), Ctrl+A (zaznacz wszystko).
- Zaznaczanie wielu plików: Ctrl+klik (pojedyncze), Shift+klik (zakres), zaznaczanie lassem (przeciągnięcie myszką po pustym miejscu).
- Pasek statusu na dole okna: liczba zaznaczonych elementów, ich łączny rozmiar, wolne miejsce na dysku.
- Kosz — usunięte pliki trafiają do kosza zamiast być kasowane od razu.
- Właściwości pliku/folderu — okno z rozmiarem, datą modyfikacji, uprawnieniami.

**Funkcje dodatkowe (do rozważenia, mogą poczekać):**
- Podgląd miniatur zdjęć w widoku ikon.
- Pasek postępu przy kopiowaniu/przenoszeniu dużych plików.
- Historia nawigacji (przyciski Wstecz/Dalej).
- Zakładki (tabs) w jednym oknie menedżera zamiast otwierania nowych okien za każdym razem.
- Cofnij (Ctrl+Z) dla usunięcia/przeniesienia plików.

**Dodatkowe funkcje wynikające z ustaleń (do zapisania):**
- Skojarzenia plików — dwuklik otwiera plik domyślną aplikacją (np. `.txt` → Notatnik, `.jpg` → podgląd zdjęć); mapowania zdefiniowane w pliku konfiguracyjnym.
- Konflikt przy kopiowaniu/przenoszeniu pliku o istniejącej nazwie: opcje nadpisz / pomiń / zmień nazwę.
- Przełącznik "pokaż ukryte pliki" (istotne na Linuksie — pliki konfiguracyjne zaczynające się od kropki).
- Zarządzanie dyskiem sieciowym (SFTP): opcje "Rozłącz" / "Zapomnij" w menu kontekstowym zasobu; obsługa zerwanego połączenia w trakcie pracy — komunikat błędu zamiast zawieszenia okna.

### Menedżer usług
- Prosta tabela: nazwa usługi / status / opis / akcje (start, stop, restart).
- Dane czytane bezpośrednio z systemd przez D-Bus (`zbus`) — bez własnej logiki zarządzania usługami, tylko UI nad gotowym mechanizmem.

### Panel sterowania
- Siatka ikon kategorii w stylu Windows XP: Sieć, Dźwięk, Wyświetlacz, Konta użytkowników, Data/godzina.
- Każda ikona to nakładka UI nad gotową usługą systemową (NetworkManager, PipeWire, UPower itd.) — panel nie implementuje logiki, tylko ją prezentuje i pozwala nią sterować.

### Ekran logowania (Greeter)
- Osobny, mniejszy compositor uruchamiany przed sesją użytkownika (`greetd` + własny frontend), spójny wizualnie z resztą systemu.

## 4. Integracja z usługami systemowymi — zasada podziału odpowiedzialności

| Kategoria | Przykłady | Kto to robi |
|---|---|---|
| Gotowe usługi, zero UI potrzebne | CUPS (drukowanie), BlueZ (Bluetooth), systemd (usługi) | Instalacja i konfiguracja systemowa — działają niezależnie od shella |
| Gotowe usługi + cienka nakładka UI pisana przez nas | NetworkManager, PipeWire, UPower | My piszemy tylko wskaźnik/kontrolkę w System Bar lub Panelu sterowania, komunikując się przez D-Bus |
| Pisane od zera | Compositor, window management, slider kart, file manager, services manager, control panel, greeter | W całości nasz kod |

**Ważne założenie:** shell nie jest odpowiedzialny za obsługę np. drukarek czy Bluetootha jako takich — to zadanie gotowych demonów systemowych. Shell dostarcza jedynie interfejs do sterowania nimi, tam gdzie to potrzebne.

## 5. Otwarte kwestie / do dalszych ustaleń

- Dokładny wygląd/rozmiar panelu przypiętej karty względem reszty ekranu (proporcje podziału).
- Dokładny format pliku skrótu w `menu_start/` (robocze założenie: `.desktop`, do potwierdzenia).
- Wybór: pisanie emulacji CPU 6510 od zera vs. wykorzystanie istniejącego silnika (rekomendacja: gotowy silnik, np. z VICE, żeby nie blokować rdzenia projektu).
- Kolejność developmentu: rekomendowane rozpoczęcie od Core na PC (x86_64), dopiero potem port na RPi3/telefon.

## 6. Kolejne kroki

1. Szkielet projektu Rust (Cargo workspace) + `smithay` jako punkt startowy compositora.
2. Proof-of-concept: pełnoekranowe okno, statyczny górny pasek, jedna karta slidera renderowana statycznie.
3. Dodanie window managementu (otwieranie/przesuwanie/zamykanie okien nad sliderem).
4. Implementacja pełnej logiki slidera (nawigacja, przypinanie, reorder, ikony funkcyjne kart).
5. Pierwsza własna aplikacja: menedżer plików (najprostszy do testowania całego stacku).
