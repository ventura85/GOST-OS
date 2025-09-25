#!/bin/bash

# Przerywa wykonanie skryptu w przypadku błędu (-e), niezdefiniowanej zmiennej (-u)
# lub błędu w potoku (-o pipefail). 'x' wyświetla każde polecenie przed wykonaniem.
set -euxo pipefail

# Aktualizacja listy pakietów w kontenerze
echo "INFO: Updating package lists..."
apt-get update

# Instalacja live-build i jego zależności. --no-install-recommends
# minimalizuje liczbę instalowanych pakietów.
echo "INFO: Installing live-build..."
apt-get install -y --no-install-recommends live-build

# Krok 1: Czyszczenie.
# Zapewnia, że budowanie zaczyna się od czystego stanu, usuwając
# wszelkie pozostałości po poprzednich przebiegach.
echo "INFO: Cleaning up previous build environment..."
lb clean --purge

# Krok 2: Konfiguracja.
# Inicjalizuje konfigurację live-build. Narzędzie automatycznie
# wczyta pliki z katalogu auto/, w tym auto/config.
echo "INFO: Configuring live-build..."
lb config

# Krok 3: Budowanie.
# Uruchamia główny proces budowania. To potrwa najdłużej.
echo "INFO: Starting the build process..."
lb build