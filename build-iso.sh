#!/bin/bash
# Wersja 2.0 FINALNA - Główny skrypt budujący ISO w kontenerze Docker

# Przerywa wykonanie skryptu w przypadku błędu
set -e

# --- KROK 1: Instalacja zależności wewnątrz kontenera ---
echo ">>> Instalowanie zależności (live-build)..."
apt-get update
apt-get install -y --no-install-recommends live-build

# --- KROK 2: Czyszczenie i konfiguracja live-build ---
echo ">>> Czyszczenie i konfiguracja środowiska live-build..."
lb clean --purge

# Używamy pełnej komendy konfiguracyjnej, aby mieć 100% pewności
lb config \
--distribution trixie \
--architectures amd64 \
--archive-areas "main contrib non-free non-free-firmware" \
--mirror-bootstrap http://deb.debian.org/debian/ \
--mirror-chroot http://deb.debian.org/debian/ \
--mirror-chroot-security http://security.debian.org/debian-security/ \
--iso-application "GOST OS" \
--iso-publisher "ventura85" \
--iso-volume "GOST-OS-1.0"

# --- KROK 3: Tworzenie niestandardowej listy pakietów ---
echo ">>> Tworzenie listy pakietów..."
cat > config/package-lists/gostos.list.chroot <<'EOF'
git
sudo
xfce4
xfce4-goodies
menulibre
mugshot
xfce4-session-wayland
librsvg2-bin
gtk2-engines-murrine
gtk2-engines-pixbuf
xfce4-appfinder
lightdm
lightdm-gtk-greeter-settings
live-boot
live-config
live-tools
linux-image-amd64
virtualbox-guest-utils
virtualbox-guest-x11
systemd-sysv
EOF

# --- KROK 4: Kopiowanie plików projektu (nakładka) ---
echo ">>> Kopiowanie plików projektu do nakładki..."
mkdir -p config/includes.chroot/opt/gost-os-project
# Kopiujemy całą zawartość bieżącego folderu (z wyjątkiem .git) do środka
rsync -a --exclude='.git/' ./ config/includes.chroot/opt/gost-os-project/

# --- KROK 5: Tworzenie skryptu 'hak' ---
echo ">>> Tworzenie skryptu 'hak' do finalnej konfiguracji..."
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

# --- KROK 6: Budowanie obrazu ISO ---
echo ">>> Rozpoczynanie właściwego budowania obrazu ISO..."
lb build