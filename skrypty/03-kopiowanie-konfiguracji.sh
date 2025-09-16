#!/bin/bash
# Wersja 7.0 - Skrypt kopiujący finalne pliki konfiguracyjne i graficzne

echo ">>> Czyszczenie domyślnych tapet Debiana..."
sudo rm -f /usr/share/images/desktop-base/desktop*

echo ">>> Kopiowanie grafiki systemowej..."
# Tworzymy dedykowany folder na naszą grafikę, żeby był porządek
sudo mkdir -p /usr/share/backgrounds/gostos

# Kopiujemy wszystkie potrzebne pliki graficzne w odpowiednie, logiczne miejsca
sudo cp zasoby/branding/wallpaper_gostos.jpg /usr/share/images/desktop-base/
sudo cp zasoby/branding/login-background.png /usr/share/backgrounds/gostos/
sudo cp zasoby/branding/avatar.png /usr/share/backgrounds/gostos/
sudo cp zasoby/branding/gostos-start.png /usr/share/pixmaps/

echo ">>> Kopiowanie plików konfiguracyjnych..."
# Używamy zmiennej $USER, aby skrypt działał dla każdego użytkownika
CONFIG_DIR="/home/$USER/.config"
DESKTOP_DIR="/home/$USER/Pulpit"

# Konfiguracja XFCE
mkdir -p $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml
cp -r konfiguracja/pulpit/* $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml/
cp -r konfiguracja/panel/* $CONFIG_DIR/xfce4/xfconf/xfce-perchannel-xml/

# Kopiowanie niestandardowej struktury menu (z menulibre)
echo ">>> Kopiowanie niestandardowej struktury menu..."
mkdir -p /home/$USER/.config/menus
mkdir -p /home/$USER/.local/share/applications
cp konfiguracja/menu/xfce-applications.menu /home/$USER/.config/menus/
cp -r konfiguracja/menu/custom-applications/* /home/$USER/.local/share/applications/

# Własne style CSS i GTK2
mkdir -p $CONFIG_DIR/gtk-3.0
cp konfiguracja/motywy/gtk.css $CONFIG_DIR/gtk-3.0/
cp konfiguracja/motywy/.gtkrc-2.0 /home/$USER/

# Konfiguracja LightDM (ekran logowania)
sudo cp konfiguracja/logowanie/lightdm-gtk-greeter.conf /etc/lightdm/

# Aktywatory pulpitu
mkdir -p $DESKTOP_DIR
cp -r konfiguracja/pulpit/skroty/* $DESKTOP_DIR
echo ">>> Ufanie aktywatorom pulpitu..."
chmod +x $DESKTOP_DIR/*.desktop

# Ustawianie awatara użytkownika dla Menu Whisker
echo ">>> Ustawianie awatara użytkownika w menu..."
cp /usr/share/backgrounds/gostos/avatar.png /home/$USER/.face
# Upewniamy się, że plik należy do użytkownika, co jest kluczowe
sudo chown $USER:$USER /home/$USER/.face

# --------------------------------------------------------------------
# SEKCJA KONFIGURACJI SYSTEMU I GRUB-A
# --------------------------------------------------------------------

# Zmiana nazwy dystrybucji
echo ">>> Zmienianie nazwy systemu na GOST OS..."
sudo cp konfiguracja/system/os-release /etc/os-release

# Konfiguracja GRUB (tło i język)
echo ">>> Konfigurowanie GRUB..."

# Krok 1: Kopiujemy nasze niestandardowe tło (w wersji 256 kolorów)
sudo cp zasoby/branding/gostos-grub-background.png /boot/grub/

# Krok 2: Modyfikujemy plik /etc/default/grub, aby używał prostego tła
# Usuwamy stare wpisy (jeśli istnieją), aby uniknąć konfliktów
sudo sed -i '/GRUB_THEME/d' /etc/default/grub
sudo sed -i '/GRUB_BACKGROUND/d' /etc/default/grub
sudo sed -i '/GRUB_LANG/d' /etc/default/grub
sudo sed -i '/GRUB_TERMINAL_OUTPUT/d' /etc/default/grub

# Dodajemy poprawne ustawienia na końcu pliku
echo '' | sudo tee -a /etc/default/grub
echo '# --- Konfiguracja dla GOST OS ---' | sudo tee -a /etc/default/grub
echo 'GRUB_BACKGROUND="/boot/grub/gostos-grub-background.png"' | sudo tee -a /etc/default/grub
echo 'GRUB_LANG="pl_PL"' | sudo tee -a /etc/default/grub
echo 'GRUB_TERMINAL_OUTPUT="gfxterm"' | sudo tee -a /etc/default/grub
echo '# ------------------------------' | sudo tee -a /etc/default/grub

# Krok 3: Aktualizujemy konfigurację GRUB-a, aby zastosować wszystkie zmiany
echo ">>> Aktualizowanie konfiguracji GRUB..."
sudo update-grub

echo ">>> Finalna konfiguracja wyglądu została zastosowana."
echo ">>> Uruchom ponownie komputer, aby zobaczyć wszystkie zmiany."