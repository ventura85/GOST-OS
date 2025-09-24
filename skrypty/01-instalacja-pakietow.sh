#!/bin/bash
# Wersja 1.5 - Skrypt instalujący pakiety dla pulpitu i ekranu logowania

apt-get update 

echo ">>> Instalowanie podstawowych narzędzi..."
sudo apt install -y git sudo

echo ">>> Instalowanie środowiska graficznego XFCE i niezbędnych narzędzi..."
sudo apt install -y xfce4 xfce4-goodies menulibre mugshot

echo ">>> Instalowanie zależności dla motywów i dodatków..."
sudo apt install -y librsvg2-bin gtk2-engines-murrine gtk2-engines-pixbuf

echo ">>> Instalowanie wyszukiwarki aplikacji..."
sudo apt install -y xfce4-appfinder

echo ">>> Instalowanie menedżera logowania LightDM..."
sudo apt install -y lightdm lightdm-gtk-greeter-settings

echo ">>> Zakończono instalację podstawowych pakietów."