#!/bin/bash
# Skrypt kopiujący finalne pliki konfiguracyjne i graficzne GOST OS

echo ">>> Czyszczenie domyślnych tapet Debiana..."
sudo rm -f /usr/share/images/desktop-base/desktop*

echo ">>> Kopiowanie grafiki systemowej..."
# Zakładamy, że ten skrypt jest uruchamiany z głównego folderu projektu
sudo cp zasoby/branding/wallpaper_gostos.jpg /usr/share/images/desktop-base/
sudo cp zasoby/branding/gostos-start.png /usr/share/pixmaps/

echo ">>> Kopiowanie plików konfiguracyjnych..."
# Używamy zmiennej $USER, aby skrypt działał dla każdego użytkownika
CONFIG_DIR="/home/$USER/.config"
DESKTOP_DIR="/home/$USER/Pulpit"

# Konfiguracja XFCE
mkdir -p $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml
cp -r konfiguracja/pulpit/*.xml $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml/
cp -r konfiguracja/panel/*.xml $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml/

# Własne style CSS i GTK2
mkdir -p $CONFIG_DIR/gtk-3.0
cp konfiguracja/motywy/gtk.css $CONFIG_DIR/gtk-3.0/
cp konfiguracja/motywy/.gtkrc-2.0 /home/$USER/

# Aktywatory pulpitu
mkdir -p $DESKTOP_DIR
cp -r konfiguracja/pulpit/skroty/* $DESKTOP_DIR

echo ">>> Ufanie aktywatorom pulpitu..."
chmod +x $DESKTOP_DIR/*.desktop

echo ">>> Finalna konfiguracja wyglądu została zastosowana."
echo ">>> Uruchom ponownie komputer, aby zobaczyć wszystkie zmiany."