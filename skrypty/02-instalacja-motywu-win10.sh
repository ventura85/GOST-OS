#!/bin/bash
# Skrypt instalujący motyw Windows 10 z lokalnych zasobów projektu

echo ">>> Tworzenie folderów na motywy..."
mkdir -p ~/.themes
mkdir -p ~/.icons

echo ">>> Kopiowanie motywu i ikon Windows 10 z lokalnych zasobów..."
# Zakładamy, że ten skrypt jest uruchamiany z folderu 'skrypty'
cp -r ../zasoby/themes/Windows-10 ~/.themes/
cp -r ../zasoby/themes/Windows-10-Icons ~/.icons/

echo ">>> Motyw i ikony zostały zainstalowane."