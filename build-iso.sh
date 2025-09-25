#!/bin/bash

# Zatrzymuje wykonanie skryptu w przypadku błędu
set -euxo pipefail

# --- KROKI WYKONYWANE WEWNĄTRZ KONTENERA DEBIAN ---

# Aktualizacja listy pakietów w kontenerze
echo "INFO: Updating package lists..."
apt-get update

# Instalacja live-build i jego zależności
echo "INFO: Installing live-build..."
apt-get install -y --no-install-recommends live-build

# Krok 1: Czyszczenie. Gwarantuje, że zaczynamy od zera.
echo "INFO: Cleaning up previous build environment..."
lb clean --purge

# Krok 2: Konfiguracja. Wczytuje ustawienia z auto/config.
echo "INFO: Configuring live-build..."
lb config

# Krok 3: Budowanie. Uruchamia właściwy proces tworzenia ISO.
echo "INFO: Starting the build process..."
lb build