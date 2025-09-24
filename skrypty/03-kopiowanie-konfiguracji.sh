#!/bin/bash
# Wersja 3.1 (dla Cubic) - Skrypt kopiujący finalne pliki konfiguracyjne

echo ">>> Kopiowanie grafiki systemowej..."
# Używamy ścieżek bezwzględnych, bo pracujemy jako root w chroot
mkdir -p /usr/share/backgrounds/gostos
cp zasoby/branding/wallpaper_gostos.jpg /usr/share/images/desktop-base/
cp zasoby/branding/login-background.png /usr/share/backgrounds/gostos/
cp zasoby/branding/avatar.png /usr/share/backgrounds/gostos/
cp zasoby/branding/gostos-start.png /usr/share/pixmaps/

echo ">>> Kopiowanie plików konfiguracyjnych..."
# W środowisku Cubic/live domyślny użytkownik to "user"
CONFIG_DIR="/home/user/.config"
DESKTOP_DIR="/home/user/Pulpit"

# Konfiguracja XFCE
mkdir -p $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml
cp -r konfiguracja/pulpit/* $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml/
cp -r konfiguracja/panel/* $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml/

# Kopiowanie niestandardowej struktury menu
echo ">>> Kopiowanie niestandardowej struktury menu..."
mkdir -p /home/user/.config/menus
mkdir -p /home/user/.local/share/applications
cp konfiguracja/menu/xfce-applications.menu /home/user/.config/menus/
cp -r konfiguracja/menu/custom-applications/* /home/user/.local/share/applications/

# Własne style CSS i GTK2
mkdir -p $CONFIG_DIR/gtk-3.0
cp konfiguracja/motywy/gtk.css $CONFIG_DIR/gtk-3.0/
cp konfiguracja/motywy/.gtkrc-2.0 /home/user/

# Konfiguracja LightDM
cp konfiguracja/logowanie/lightdm-gtk-greeter.conf /etc/lightdm/

# Aktywatory pulpitu
mkdir -p $DESKTOP_DIR
cp -r konfiguracja/pulpit/skroty/* $DESKTOP_DIR
echo ">>> Ufanie aktywatorom pulpitu..."
chmod +x $DESKTOP_DIR/*.desktop

# Ustawianie awatara użytkownika
echo ">>> Ustawianie awatara użytkownika w menu..."
cp /usr/share/backgrounds/gostos/avatar.png /home/user/.face
chown 1000:1000 /home/user/.face

echo ">>> Finalna konfiguracja wyglądu (wewnątrz ISO) została zastosowana."