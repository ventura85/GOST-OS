#!/usr/bin/env bash
# Zwalnia skróty klawiszowe, które XFCE przechwytuje, zanim dotrą do kompozytora
# uruchomionego jako okno w sesji (docs/01-strategia-dev-test.md §2.2, Tier 1).
#
# Dlaczego to w ogóle istnieje: powłoka GostUI posiada modyfikator Super (D-041),
# a xfwm4 i xfsettingsd trzymają na nim kilka własnych skrótów. Klawisz łapany
# przez sesję-gospodarza nie dociera do zagnieżdżonego okna w ogóle, więc
# `Super+Tab` wygląda jak nasza usterka, a `Super+F` zamiast pełnego ekranu
# otwiera menedżer plików. Na gołym metalu (M4) problem nie istnieje — to
# ograniczenie warsztatu, nie produktu, i dlatego rozwiązuje się je w konfiguracji
# warsztatu, a nie zmianą skrótów w kodzie.
#
#   ./scripts/xfce-zwolnij-skroty.sh            # zwolnij
#   ./scripts/xfce-zwolnij-skroty.sh --przywroc # przywróć ustawienia XFCE
#
# Skrypt zapisuje poprzednie wartości, więc przywracanie oddaje dokładnie to, co
# było, a nie to, co jest domyślne w XFCE.

set -euo pipefail

KANAL="xfce4-keyboard-shortcuts"
KOPIA="${XDG_CACHE_HOME:-$HOME/.cache}/gostui-skroty-xfce.txt"

# Skróty, które kolidują z D-041. Lista jest jawna i krótka celowo: zwalnianie
# wszystkiego, co ma Super, zabrałoby użytkownikowi jego własne ustawienia.
SKROTY=(
  "/xfwm4/custom/<Super>Tab"
  "/xfwm4/custom/<Super>Left"
  "/xfwm4/custom/<Super>Right"
  "/xfwm4/custom/<Super>Up"
  "/xfwm4/custom/<Super>Down"
  "/commands/custom/<Super>f"
  # D-048: tryb edycji kart. XFCE ma tu menedżera plików, więc bez zwolnienia
  # `Super+E` w oknie zagnieżdżonym otwiera się Thunar i nic do nas nie dociera —
  # dokładnie ten sam objaw co przy `Super+F`.
  "/commands/custom/<Super>e"
)

if ! command -v xfconf-query >/dev/null; then
  echo "Brak xfconf-query — to nie jest sesja XFCE. Nic nie robię." >&2
  exit 1
fi

if [[ "${1:-}" == "--przywroc" ]]; then
  if [[ ! -f "$KOPIA" ]]; then
    echo "Brak kopii ($KOPIA) — nie ma czego przywracać." >&2
    exit 1
  fi
  while IFS=$'\t' read -r wlasciwosc wartosc; do
    [[ -z "$wlasciwosc" ]] && continue
    xfconf-query -c "$KANAL" -p "$wlasciwosc" --create -t string -s "$wartosc"
    echo "przywrócono: $wlasciwosc = $wartosc"
  done < "$KOPIA"
  rm -f "$KOPIA"
  echo "Gotowe. Skróty XFCE wróciły na miejsce."
  exit 0
fi

: > "$KOPIA"
zwolnione=0
for wlasciwosc in "${SKROTY[@]}"; do
  # `|| true`: brak skrótu nie jest błędem — użytkownik mógł go już usunąć albo
  # nigdy go nie mieć, a skrypt ma być bezpieczny do uruchomienia dwa razy.
  wartosc="$(xfconf-query -c "$KANAL" -p "$wlasciwosc" 2>/dev/null || true)"
  if [[ -z "$wartosc" ]]; then
    echo "pominięto (nie ustawiony): $wlasciwosc"
    continue
  fi
  printf '%s\t%s\n' "$wlasciwosc" "$wartosc" >> "$KOPIA"
  xfconf-query -c "$KANAL" -p "$wlasciwosc" -r
  echo "zwolniono: $wlasciwosc (było: $wartosc)"
  zwolnione=$((zwolnione + 1))
done

echo
echo "Zwolniono $zwolnione skrótów. Kopia poprzednich wartości: $KOPIA"
echo "Przywrócenie: $0 --przywroc"
