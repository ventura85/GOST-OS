#!/bin/bash
# Wersja 4.0 - Skrypt instalujący motywy z lokalnych zasobów projektu
# WAŻNE: Ten skrypt należy uruchomić z głównego folderu projektu GOST-OS!

echo ">>> Instalowanie motywu okien WhiteSur z lokalnych zasobów..."
sudo ./zasoby/WhiteSur-gtk-theme/install.sh -t green -l

echo ">>> Instalowanie ikon Win10Sur z lokalnych zasobów..."
./zasoby/Win10Sur-icon-theme/install.sh -a

echo ">>> Motywy zostały zainstalowane z lokalnego repozytorium."