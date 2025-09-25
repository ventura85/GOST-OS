#!/bin/bash
# Przerywa wykonanie skryptu w przypadku błędu
set -e

echo ">>> Krok 1: Czyszczenie starej budowy (jeśli istnieje)..."
lb clean --purge

echo ">>> Krok 2: Konfiguracja live-build..."
# Inicjalizuje konfigurację. Ustawienia zostaną wczytane z auto/config
lb config

# --- Tutaj dodajemy naszą personalizację do świeżej konfiguracji ---

echo ">>> Krok 3: Dodawanie niestandardowej listy pakietów..."
# Tworzymy listę pakietów GOST OS
cat > config/package-lists/gostos.list.chroot <<'EOF'
# Pakiety systemowe i narzędzia
git
sudo
# Środowisko graficzne i aplikacje
xfce4
xfce4-goodies
menulibre
mugshot
xfce4-session-wayland
lightdm
lightdm-gtk-greeter-settings
# Zależności i sterowniki
gtk2-engines-murrine
virtualbox-guest-utils
virtualbox-guest-x11
systemd-sysv
EOF

echo ">>> Krok 4: Kopiowanie plików projektu (nakładka)..."
# Kopiujemy całą zawartość repozytorium do folderu wewnątrz ISO
mkdir -p config/includes.chroot/opt/gost-os-project
rsync -a --exclude='.git/' --exclude='.github/' ./ config/includes.chroot/opt/gost-os-project/

echo ">>> Krok 5: Tworzenie skryptu 'hak' do finalnej konfiguracji..."
# Tworzymy skrypt, który uruchomi się wewnątrz budowanego systemu
mkdir -p config/hooks/chroot
cat > config/hooks/chroot/99-gost-os-setup.hook.chroot <<'EOF'
#!/bin/bash
set -e
cd /opt/gost-os-project/skrypty
chmod +x *.sh
chmod +x ../zasoby/WhiteSur-gtk-theme/install.sh
chmod +x ../zasoby/Win10Sur-icon-theme/install.sh

echo ">>> Uruchamianie skryptów instalacyjnych GOST OS..."
./01-instalacja-pakietow.sh
./02-instalacja-motywow.sh
./03-kopiowanie-konfiguracji.sh

echo ">>> Czyszczenie po instalacji..."
rm -rf /opt/gost-os-project
EOF
chmod +x config/hooks/chroot/99-gost-os-setup.hook.chroot

echo ">>> Krok 6: Rozpoczynanie właściwego budowania obrazu ISO..."
lb build