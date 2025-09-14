#!/bin/bash
# Wersja 2.0 - Skrypt kopiujący finalne pliki konfiguracyjne i graficzne

echo ">>> Czyszczenie domyślnych tapet Debiana..."
sudo rm -f /usr/share/images/desktop-base/desktop*

echo ">>> Kopiowanie grafiki systemowej..."
# Zakładamy, że ten skrypt jest uruchamiany z głównego folderu projektu
sudo cp zasoby/branding/wallpaper_gostos.jpg /usr/share/images/desktop-base/
sudo cp zasoby/branding/login-background.png /usr/share/backgrounds/gostos/
sudo cp zasoby/branding/gostos-start.png /usr/share/pixmaps/

# Podmiana domyślnego awatara systemowego (metoda "hakerska")
AVATAR_PATH="/usr/share/icons/Adwaita/scalable/status/avatar-default.svg"
if [ -f "$AVATAR_PATH" ]; then
    sudo mv "$AVATAR_PATH" "$AVATAR_PATH.kopia"
fi
sudo cp zasoby/branding/avatar.png "$AVATAR_PATH"

echo ">>> Kopiowanie plików konfiguracyjnych..."
# Używamy zmiennej $USER, aby skrypt działał dla każdego użytkownika
CONFIG_DIR="/home/$USER/.config"
DESKTOP_DIR="/home/$USER/Pulpit"

# Konfiguracja XFCE
mkdir -p $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml
cp -r konfiguracja/pulpit/* $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml/
cp -r konfiguracja/panel/* $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml/

# Własne style CSS i GTK2
mkdir -p $CONFIG_DIR/gtk-3.0
cp konfiguracja/motywy/gtk.css $CONFIG_DIR/gtk-3.0/
cp konfiguracja/motywy/.gtkrc-2.0 /home/$USER/

# Konfiguracja LightDM
sudo cp konfiguracja/logowanie/lightdm-gtk-greeter.conf /etc/lightdm/

# Aktywatory pulpitu
mkdir -p $DESKTOP_DIR
cp -r konfiguracja/pulpit/skroty/* $DESKTOP_DIR
echo ">>> Ufanie aktywatorom pulpitu..."
chmod +x $DESKTOP_DIR/*.desktop

echo ">>> Finalna konfiguracja wyglądu została zastosowana."
echo ">>> Uruchom ponownie komputer, aby zobaczyć wszystkie zmiany."