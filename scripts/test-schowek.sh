#!/usr/bin/env bash
# Kopiuj-wklej między dwoma klientami, w obie strony — kryterium M2.
#
# Dlaczego skrypt, a nie zaznaczenie myszą w dwóch terminalach: test klikany nie
# jest powtarzalny i nikt go nie powtórzy przy następnej zmianie w routingu
# wejścia. Schowek zależy od fokusu klawiatury, więc psuje go każda zmiana w
# `refresh_keyboard_focus` — a psuje po cichu, bo wklejenie niczego wygląda jak
# pusty schowek.
#
# Pułapka, na której ten test już raz stanął: `wl-copy` po ustawieniu selekcji
# **forkuje demona**, który dziedziczy stdout. Uruchomiony w potoku (`| tail`,
# `$(...)`) nie kończy się nigdy, bo potok czeka na demona trzymającego schowek.
# Dlatego każde wywołanie tutaj przekierowuje wyjście do pliku, nie do potoku.
#
# Użycie: kompozytor w drugim terminalu, potem
#   WAYLAND_DISPLAY=wayland-gostui scripts/test-schowek.sh

set -uo pipefail

: "${WAYLAND_DISPLAY:=wayland-gostui}"
export WAYLAND_DISPLAY

for narzedzie in wl-copy wl-paste; do
    if ! command -v "$narzedzie" >/dev/null; then
        echo "brak $narzedzie — zainstaluj pakiet wl-clipboard" >&2
        exit 1
    fi
done

TMP=$(mktemp -d)
# Sprzątamy wyłącznie po sobie. Demony `wl-copy` zostawione przez rundy giną
# same, gdy kompozytor się kończy — a `pkill wl-copy` zabiłby też schowek
# w innej sesji wayland tego użytkownika, czyli cudze dane za nasz porządek.
trap 'rm -rf "$TMP"' EXIT

bledy=0

# Jedna runda: ustaw schowek, odczytaj go i porównaj z oczekiwaniem.
#
# Każde wywołanie to nowy proces, czyli naprawdę dwa różne klienty wayland —
# o to chodzi w kryterium „między dwoma klientami", a nie o dwa okna tej samej
# aplikacji.
runda() {
    local opis="$1" tekst="$2" flaga="${3:-}"
    printf '%s' "$tekst" | timeout 10 wl-copy $flaga >"$TMP/copy.log" 2>&1
    local kod_copy=$?
    sleep 0.5
    timeout 10 wl-paste $flaga >"$TMP/wynik.txt" 2>"$TMP/paste.err"
    local kod_paste=$?
    local odczyt
    odczyt=$(cat "$TMP/wynik.txt")

    if [ "$kod_copy" -ne 0 ]; then
        printf '%-34s NIE UDAŁO SIĘ (wl-copy kod %s)\n' "$opis" "$kod_copy"
        bledy=$((bledy + 1))
    elif [ "$kod_paste" -ne 0 ]; then
        printf '%-34s NIE UDAŁO SIĘ (wl-paste kod %s)\n' "$opis" "$kod_paste"
        bledy=$((bledy + 1))
    elif [ "$odczyt" != "$tekst" ]; then
        printf '%-34s NIE ZGADZA SIĘ: [%s] zamiast [%s]\n' "$opis" "$odczyt" "$tekst"
        bledy=$((bledy + 1))
    else
        printf '%-34s ok [%s]\n' "$opis" "$odczyt"
    fi
}

echo "Gniazdo: $WAYLAND_DISPLAY"
echo

runda "schowek: klient A → klient B" "tekst-od-A"
runda "schowek: klient B → klient A" "tekst-od-B"
# Trzecia runda nie jest powtórzeniem drugiej: sprawdza, że nowe źródło
# **odbiera** schowek poprzedniemu. Kompozytor, który zapamiętuje pierwszą
# selekcję i ignoruje kolejne, przechodzi dwie pierwsze rundy i psuje się przy
# drugim kopiowaniu w sesji.
runda "schowek: przejęcie przez trzeciego" "tekst-trzeci"

# Wayland trzyma zaznaczenie spod środkowego przycisku osobno od schowka i
# użytkownicy polegają na tej różnicy. Osobny stan w kompozytorze, więc osobny
# test — sprawny schowek nie mówi nic o primary.
runda "primary: klient A → klient B" "primary-od-A" "--primary"
runda "primary: klient B → klient A" "primary-od-B" "--primary"

echo
if [ "$bledy" -eq 0 ]; then
    echo "Kopiuj-wklej działa w obie strony, schowek i primary."
    exit 0
fi
echo "$bledy z 5 rund nie przeszło."
exit 1
