#!/bin/bash
# Skrypt kopiujący finalne pliki konfiguracyjne i graficzne GOST OS

echo ">>> Czyszczenie domyślnych tapet Debiana..."
sudo rm -f /usr/share/images/desktop-base/desktop*

echo ">>> Kopiowanie plików konfiguracyjnych pulpitu i panelu..."
# Zakładamy, że ten skrypt jest uruchamiany z folderu 'skrypty' i pliki są w folderze domowym
# Używamy zmiennej $USER, aby skrypt działał dla każdego użytkownika
CONFIG_DIR="/home/$USER/.config/xfce4/xfconf/xfce-perchannel-xml"
mkdir -p $CONFIG_DIR
cp -r ../konfiguracja/pulpit/* $CONFIG_DIR
cp -r ../konfiguracja/panel/* $CONFIG_DIR

echo ">>> Kopiowanie grafiki systemowej..."
sudo cp ../zasoby/branding/wallpaper_gostos.jpg /usr/share/images/desktop-base/
sudo cp ../zasoby/branding/gostos-start.png /usr/share/pixmaps/

echo ">>> Finalna konfiguracja wyglądu została zastosowana."
echo ">>> Uruchom ponownie komputer, aby zobaczyć wszystkie zmiany."