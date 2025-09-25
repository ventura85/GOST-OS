#!/bin/bash
# Wersja 3.2 FINALNA - Skrypt kopiujący finalne pliki konfiguracyjne z naprawą uprawnień

echo ">>> Kopiowanie grafiki systemowej..."
mkdir -p /usr/share/backgrounds/gostos
cp zasoby/branding/wallpaper_gostos.jpg /usr/share/images/desktop-base/
cp zasoby/branding/login-background.png /usr/share/backgrounds/gostos/
cp zasoby/branding/avatar.png /usr/share/backgrounds/gostos/
cp zasoby/branding/gostos-start.png /usr/share/pixmaps/

echo ">>> Kopiowanie plików konfiguracyjayjnych..."
# W środowisku Cubic/live domyślny użytkownik to "user" o ID 1000
USER_HOME="/home/user"
CONFIG_DIR="$USER_HOME/.config"
DESKTOP_DIR="$USER_HOME/Pulpit"

# Konfiguracja XFCE
mkdir -p $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml
cp -r konfiguracja/pulpit/* $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml/
cp -r konfiguracja/panel/* $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml/

# Kopiowanie niestandardowej struktury menu
echo ">>> Kopiowanie niestandardowej struktury menu..."
mkdir -p $USER_HOME/.config/menus
mkdir -p $USER_HOME/.local/share/applications
cp konfiguracja/menu/xfce-applications.menu $USER_HOME/.config/menus/
cp -r konfiguracja/menu/custom-applications/* $USER_HOME/.local/share/applications/

# Własne style CSS i GTK2
mkdir -p $CONFIG_DIR/gtk-3.0
cp konfiguracja/motywy/gtk.css $CONFIG_DIR/gtk-3.0/
cp konfiguracja/motywy/.gtkrc-2.0 $USER_HOME/

# Konfiguracja LightDM
cp konfiguracja/logowanie/lightdm-gtk-greeter.conf /etc/lightdm/

# Aktywatory pulpitu
mkdir -p $DESKTOP_DIR
cp -r konfiguracja/pulpit/skroty/* $DESKTOP_DIR
echo ">>> Ufanie aktywatorom pulpitu..."
chmod +x $DESKTOP_DIR/*.desktop

echo ">>> Naprawianie uprawnień w folderze domowym użytkownika..."
# Ustawiamy awatar
cp /usr/share/backgrounds/gostos/avatar.png $USER_HOME/.face
# Rekursywnie zmieniamy właściciela wszystkich plików i folderów, które stworzyliśmy
chown -R 1000:1000 "$USER_HOME"

echo ">>> Finalna konfiguracja wyglądu (wewnątrz ISO) została zastosowana."