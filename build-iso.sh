#!/bin/bash
# Przerywa wykonanie skryptu w przypadku błędu
set -e

# --- KROK 1: Instalacja zależności wewnątrz kontenera ---
echo ">>> Instalowanie zależności (live-build, rsync)..."
apt-get update
apt-get install -y --no-install-recommends live-build rsync

# --- KROK 2: Czyszczenie ---
echo ">>> Czyszczenie starej budowy (jeśli istnieje)..."
lb clean --purge

# --- KROK 3: Konfiguracja ---
echo ">>> Konfiguracja live-build..."
# Inicjalizuje konfigurację. Ustawienia zostaną wczytane z auto/config
lb config

# --- KROK 4: Dodanie niestandardowej personalizacji ---
echo ">>> Dodawanie niestandardowej listy pakietów..."
cp -r auto/package-lists/* config/package-lists/

echo ">>> Kopiowanie plików projektu (nakładka)..."
mkdir -p config/includes.chroot/opt/gost-os-project
rsync -a --exclude='.git/' --exclude='.github/' --exclude='auto/' ./ config/includes.chroot/opt/gost-os-project/

echo ">>> Tworzenie skryptu 'hak' do finalnej konfiguracji..."
cp -r auto/hooks/* config/hooks/
chmod -R +x config/hooks

# --- KROK 5: Budowanie obrazu ISO ---
echo ">>> Rozpoczynanie właściwego budowania obrazu ISO..."
lb build