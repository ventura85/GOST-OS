#!/bin/bash
# Wersja 3.0 - Skrypt instalujący motywy z lokalnych zasobów projektu GOST OS

echo ">>> Instalowanie motywu okien WhiteSur z lokalnych zasobów..."
# Zakładamy, że skrypt jest w folderze 'skrypty'
cd ../zasoby/themes/WhiteSur-gtk-theme
sudo ./install.sh -t green -l
cd ../../../skrypty/ # Wróć do folderu ze skryptami

echo ">>> Instalowanie ikon Win10Sur z lokalnych zasobów..."
cd ../zasoby/icons/Win10Sur-icon-theme
./install.sh -a
cd ../../../skrypty/ # Wróć do folderu ze skryptami

echo ">>> Motywy zostały zainstalowane z lokalnego repozytorium."