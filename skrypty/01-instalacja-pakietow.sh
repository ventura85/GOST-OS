#!/bin/bash
# Wersja 1.3 - Skrypt instalujący pakiety dla pulpitu i ekranu logowania

echo ">>> Instalowanie podstawowych narzędzi..."
sudo apt install -y git sudo

echo ">>> Instalowanie środowiska graficznego XFCE..."
sudo apt install -y xfce4 xfce4-goodies

echo ">>> Instalowanie zależności dla motywów i dodatków..."
sudo apt install -y librsvg2-bin gtk2-engines-murrine gtk2-engines-pixbuf

echo ">>> Instalowanie wyszukiwarki aplikacji..."
sudo apt install -y xfce4-appfinder

echo ">>> Instalowanie menedżera logowania LightDM..."
sudo apt install -y lightdm lightdm-gtk-greeter-settings

echo ">>> Zakończono instalację podstawowych pakietów."